//! presentation/middleware/authz.rs — 入口の認可(シャドーモード対応)。DEMO-000173 PR 4。
//!
//! employee_routes の各リクエストを、ルート表(`route_table`)に照らして判定する。
//! 当面は Shadow(判定して記録するだけで、通す)。強制は `AUTHZ_MODE=enforce` と
//! `AUTHZ_ENFORCE_DOMAINS` で、領域ごとに段階的に行う。範囲(どの組織のデータか)の判定はハンドラ内。
//! 詳細: docs/design/詳細設計書_ロールと組織に基づく閲覧範囲.md §4.1, §4.2

use std::sync::LazyLock;

use axum::{
    extract::{MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sqlx::PgPool;

use super::role::AuthUser;
use crate::domain::authz::Permission;
use crate::infrastructure::repositories::authz_repo;
use crate::presentation::route_table::{lookup, Rule};

/// 認可ミドルウェアの動作モード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthzMode {
    /// 何もしない。
    Off,
    /// 判定して記録するだけ。常に通す(既定)。
    Shadow,
    /// 強制する(対象領域のみ。`enforce_domains`)。
    Enforce,
}

/// 認可ミドルウェアの設定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzConfig {
    pub mode: AuthzMode,
    /// 強制する領域(例: "settings", "partner")。"*" は全て(表に無いルートを含む)。
    pub enforce_domains: Vec<String>,
}

impl AuthzConfig {
    /// 環境変数の値から設定を作る。モードが無い・不明なら Shadow。
    pub fn from_values(mode: Option<&str>, domains: Option<&str>) -> AuthzConfig {
        let mode = match mode.map(|m| m.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => AuthzMode::Off,
            Some("enforce") => AuthzMode::Enforce,
            Some("shadow") | None | Some("") => AuthzMode::Shadow,
            Some(other) => {
                tracing::warn!("AUTHZ_MODE の値が不明です({other})。shadow として動かします");
                AuthzMode::Shadow
            }
        };
        let enforce_domains = domains
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        AuthzConfig { mode, enforce_domains }
    }

    fn from_env() -> AuthzConfig {
        let mode = std::env::var("AUTHZ_MODE").ok();
        let domains = std::env::var("AUTHZ_ENFORCE_DOMAINS").ok();
        Self::from_values(mode.as_deref(), domains.as_deref())
    }

    fn enforces(&self, domain: &str) -> bool {
        self.enforce_domains.iter().any(|d| d == domain)
    }
}

static CONFIG: LazyLock<AuthzConfig> = LazyLock::new(AuthzConfig::from_env);

/// 入口の判定結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryDecision {
    Allow,
    Deny,
}

/// 入口の判定(ルート表の Rule と、ユーザーが権限を持つか)。範囲の判定はハンドラ内。
/// 表に無いルート(None)は、管理者を含め拒否する(deny-by-default。表の完全性はテストが保証する)。
pub fn entry_decision(rule: Option<Rule>, is_admin: bool, has_perm: impl Fn(Permission) -> bool) -> EntryDecision {
    let Some(rule) = rule else { return EntryDecision::Deny };
    if is_admin {
        return EntryDecision::Allow;
    }
    match rule {
        Rule::Perm(p) => {
            if has_perm(p) {
                EntryDecision::Allow
            } else {
                EntryDecision::Deny
            }
        }
        Rule::AdminOnly => EntryDecision::Deny,
        Rule::OwnOr(_) | Rule::Own | Rule::Authenticated => EntryDecision::Allow,
    }
}

/// モードに応じた扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// そのまま通す。
    Pass,
    /// 通すが、強制したら拒否されることを記録する。
    PassButLogDiff,
    /// 403 で拒否する。
    Reject,
}

/// モードと判定から、扱いを決める。Shadow は決して Reject しない。
pub fn outcome(cfg: &AuthzConfig, rule: Option<Rule>, decision: EntryDecision) -> Outcome {
    if cfg.mode == AuthzMode::Off || decision == EntryDecision::Allow {
        return Outcome::Pass;
    }
    if cfg.mode == AuthzMode::Shadow {
        return Outcome::PassButLogDiff;
    }
    let enforced = match rule {
        Some(Rule::Perm(p)) => cfg.enforces(p.domain()),
        // 管理者専用ルートは、権限コードのドメイン(system など)と分けて "admin_only" で強制する
        Some(Rule::AdminOnly) => cfg.enforces("admin_only"),
        Some(_) => false,
        None => cfg.enforces("*"),
    };
    if enforced {
        Outcome::Reject
    } else {
        Outcome::PassButLogDiff
    }
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "success": false, "error": "権限がありません" }))).into_response()
}

/// 入口の認可ミドルウェア。`AuthUser` が入っていること(認証ミドルウェアの内側)が前提。
pub async fn authz_entry(State(pool): State<PgPool>, mut request: Request, next: Next) -> Response {
    let cfg: &AuthzConfig = &CONFIG;
    if cfg.mode == AuthzMode::Off {
        return next.run(request).await;
    }
    let Some(user) = request.extensions().get::<AuthUser>().cloned() else {
        return next.run(request).await; // 未認証は既存のガードが扱う
    };
    let method = request.method().as_str().to_string();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let rule = lookup(&method, &path).map(|s| s.rule);
    let is_admin = user.is_admin();

    // 権限が要るときだけ DB から読む。失敗しても 500 にせず「権限なし」として扱う。
    // `OwnOr` も読む: 入口は通るが、ハンドラが「全員分を見てよいか」(給与・社員情報・経費。PR 8)を権限で判断するため。
    let mut loaded = None;
    if !is_admin && matches!(rule, Some(Rule::Perm(_)) | Some(Rule::OwnOr(_))) {
        match authz_repo::load_authz(&pool, user.user.id, user.employee_id(), user.user.is_staff).await {
            Ok(a) => loaded = Some(a),
            Err(e) => tracing::error!(user_id = user.user.id, "権限情報の読み込みに失敗しました(権限なしとして扱います): {e:#}"),
        }
    }
    let decision = entry_decision(rule, is_admin, |p| loaded.as_ref().is_some_and(|a| a.has_any(p)));
    let code = match rule {
        Some(Rule::Perm(p)) | Some(Rule::OwnOr(p)) => p.code(),
        _ => "-",
    };
    match outcome(cfg, rule, decision) {
        Outcome::Pass => {}
        Outcome::PassButLogDiff => {
            if rule.is_none() {
                tracing::warn!(target: "authz_shadow", user_id = user.user.id, %method, %path, "AUTHZ_ROUTE_MISSING: ルート表に無いルートです");
            } else {
                tracing::info!(target: "authz_shadow", user_id = user.user.id, %method, %path, permission = code, "AUTHZ_SHADOW_DIFF: 強制すると拒否される");
            }
        }
        Outcome::Reject => {
            tracing::warn!(target: "authz_shadow", user_id = user.user.id, %method, %path, permission = code, "AUTHZ_DENIED: 権限がないため拒否しました");
            return forbidden();
        }
    }
    if let Some(a) = loaded {
        request.extensions_mut().insert(a);
    }
    next.run(request).await
}

#[cfg(test)]
#[path = "authz_tests.rs"]
mod tests;
