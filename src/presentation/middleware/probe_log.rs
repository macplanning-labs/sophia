/// presentation/middleware/probe_log.rs — 外部向けトークンリンクへの無効な試行を記録し、続いたら知らせる
///
/// `/token/*` `/invite/*` `/auth/portal-link/*` `/mobile-upload/*`(`/api/v1/` 付きを含む)への、
/// 無効なトークンの試行(UUID でない・存在しない)を1件ずつ WARN で記録する。
/// APIキー認証の失敗(api_key_auth)も、同じ数え方・同じ閾値・同じ通知で扱う(DEMO-000181 項目6・7)。同じIPが短時間に続けた、または全体で急に増えたときだけ、ERROR と
/// Google Chat で1回知らせる(正常なら通知しない)。判定の本体は domain::services::probe_monitor。
/// リクエストの扱いは変えない(ブロックしない)。DEMO-000167 項目6。
///
/// ログにはトークン・パス・本文を出さない(トークンは認証情報のため)。出すのは入口の種類・IP・状態コード・件数だけ。

use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::domain::services::chat_notifier;
use crate::domain::services::probe_monitor::{
    classify, is_invalid_probe, EntryKind, Outcome, ProbeMonitor, GLOBAL_KEY, GLOBAL_THRESHOLD, PER_IP_THRESHOLD,
    WINDOW,
};
use crate::presentation::auth_messages::{client_ip, peer_of};

/// ハンドラが応答に付ける目印。状態コードだけでは「無効なトークン」と分からない入口
/// (例: ポータルのマジックリンクは、存在しない/期限切れでも 200 や 410 で返す)が、試行として数えてもらうために使う。
#[derive(Debug, Clone, Copy)]
pub struct InvalidTokenAttempt;

/// APIキー認証の失敗を数える監視(トークンの試行とは別に数える)
static API_KEY_MONITOR: LazyLock<ProbeMonitor> = LazyLock::new(ProbeMonitor::new);

/// APIキー認証の失敗を1件記録し、続いたときだけ知らせる。ブロックはしない。
/// ログにはキー・ハッシュを出さない(IP と件数だけ)。
pub fn record_api_key_failure(ip: &str) {
    note_invalid_attempt(&API_KEY_MONITOR, EntryKind::ApiKey, ip, 401, Instant::now());
}

/// 無効な試行を1件数えて記録し、閾値に達したときだけ通知する(probe_log と APIキー認証で共通)。
/// 戻り値は (IP別, 全体) の数え結果。
pub fn note_invalid_attempt(
    monitor: &ProbeMonitor,
    kind: EntryKind,
    ip: &str,
    status: u16,
    now: Instant,
) -> (Outcome, Outcome) {
    let per_ip = monitor.record(ip, PER_IP_THRESHOLD, now);
    let global = monitor.record(GLOBAL_KEY, GLOBAL_THRESHOLD, now);

    tracing::warn!(
        "[外部入口/無効トークン] 処理=トークン照合 結果=無効 影響=なし(探られている可能性) | 入口={} ip={} status={} ip別件数={}",
        kind.label(), ip, status, per_ip.count
    );

    if per_ip.newly_alerted {
        alert(
            format!("{}の連続を検知しました（入口={} IP={} {}件/{}分）", kind.subject(), kind.label(), ip, per_ip.count, WINDOW.as_secs() / 60),
            "アクセスはブロックしていません。取引先の誤操作でなければ、探られている可能性があります",
            "同じ IP が続く場合は、アクセス元を確認してください",
        );
    }
    if global.newly_alerted {
        alert(
            format!("{}が、全体で{}件/{}分に達しました（入口={}）", kind.subject(), global.count, WINDOW.as_secs() / 60, kind.label()),
            "アクセスはブロックしていません。IP を変えて探られている可能性があります",
            "ログの [外部入口/無効トークン] を確認してください",
        );
    }
    (per_ip, global)
}

pub async fn probe_log(State(monitor): State<Arc<ProbeMonitor>>, req: Request, next: Next) -> Response {
    // 対象外のパスは、何もせず通す(ここは全リクエストが通るので、軽く済ませる)
    let target = classify(req.uri().path()).map(|(kind, seg)| (kind, seg.to_string()));
    let Some((kind, segment)) = target else {
        return next.run(req).await;
    };
    let ip = client_ip(peer_of(&req), req.headers());

    let res = next.run(req).await;
    let status = res.status().as_u16();
    let marked = res.extensions().get::<InvalidTokenAttempt>().is_some();
    if !marked && !is_invalid_probe(&segment, status) {
        return res;
    }

    note_invalid_attempt(&monitor, kind, &ip, status, Instant::now());
    res
}

/// ERROR ログ + Chat 通知。通知の失敗でリクエストを遅らせない(別タスクで送る)。
fn alert(summary: String, impact: &'static str, action: &'static str) {
    tracing::error!("[外部入口/無効トークン] 処理=連続試行の検知 結果=閾値超過 影響=攻撃の兆候の可能性 | {}", summary);
    let incident = chat_notifier::Incident::new(integration_core::Level::Warning, summary, impact, action);
    tokio::spawn(async move {
        chat_notifier::post(&incident).await;
    });
}
