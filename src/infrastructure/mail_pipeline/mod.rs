/// infrastructure/mail_pipeline — メール自動取込パイプライン（4フェーズ疎結合設計）
///
/// Phase1(監視・分類) → Phase2(データソース取得) → Phase3(解析・登録) → Phase4(保存・通知)
/// 各フェーズは t_received_email.status を経由して疎結合に連携する。
/// 詳細は各サブモジュールのドキュメントコメントを参照。
///
/// `run_pipeline()` が唯一のオーケストレーター。スケジューラ(15分毎を想定)と、
/// ダッシュボードの手動トリガーAPIの両方から同じ関数を呼ぶことで、自動/手動でロジックが
/// 分岐・重複しないようにする。

pub mod checkpoint;
pub mod circuit_breaker;
pub mod imap_util;
pub mod ops_message;
pub mod phase1_watch;
pub mod phase2_fetch;
pub mod phase3_parse_register;
pub mod phase4_store_notify;

use sqlx::PgPool;

/// パイプライン全体の実行結果（ログ・手動トリガーAPIのレスポンス用）
#[derive(Debug, Default)]
pub struct PipelineRunResult {
    pub phase1: Option<phase1_watch::Phase1Result>,
    pub phase1_fatal_error: Option<String>,
    pub phase2: phase2_fetch::Phase2Result,
    pub phase3: phase3_parse_register::Phase3Result,
    pub phase4: phase4_store_notify::Phase4Result,
}

impl std::fmt::Display for PipelineRunResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.phase1 {
            Some(p1) => write!(f, "Phase1[{p1}] ")?,
            None => write!(f, "Phase1[致命的エラー: {}] ", self.phase1_fatal_error.as_deref().unwrap_or(""))?,
        }
        write!(f, "Phase2[{}] Phase3[{}] Phase4[{}]", self.phase2, self.phase3, self.phase4)
    }
}

impl PipelineRunResult {
    /// いずれかのフェーズでエラーが発生したか（手動トリガーAPIのok/errレスポンス判定用）
    pub fn has_errors(&self) -> bool {
        self.phase1_fatal_error.is_some()
            || !self.phase2.errors.is_empty()
            || !self.phase3.errors.is_empty()
            || !self.phase4.errors.is_empty()
    }
}

/// パイプライン全体のエントリポイント。
///
/// 各フェーズは独立に実行し、あるフェーズが0件・エラーでも後続フェーズの実行は妨げない
/// （Phase2以降は前回までにDBに溜まった未処理行を拾えるため、Phase1が失敗しても
/// 　パイプライン全体が完全に停止することはない）。
///
/// Phase1のIMAP接続/認証エラーのみ「運用が完全に止まる致命的エラー」として、
/// Phase4の集約通知を待たずにこの場でADMINへ即時アラートを送る。
///
/// `override_month`: `Some((year, month))` の場合、Phase2のEDI-OASIS APIポーリングを
/// その月のみに限定する（ダッシュボードで年月を指定した手動一括取込用）。`None` なら
/// 従来通り「当月+翌月」を自動ポーリングする（スケジューラ・通常の手動トリガー用）。
pub async fn run_pipeline(pool: &PgPool, override_month: Option<(i32, i32)>) -> PipelineRunResult {
    let mut result = PipelineRunResult::default();

    match run_phase1_with_breaker(pool).await {
        Phase1Outcome::Success(p1) => {
            tracing::info!("[Pipeline] Phase1完了: {p1}");
            // 個別メールの非致命エラー（一覧書込失敗など）を監視へ流す
            ops_message::log_errors(&p1.errors);
            result.phase1 = Some(p1);
        }
        Phase1Outcome::LockedSkip(msg) => {
            // ロック確定時点で既にADMINへ通知済みのため、スキップの度に再アラートはしない
            tracing::warn!("[Pipeline] {msg}");
            result.phase1_fatal_error = Some(msg);
        }
        Phase1Outcome::Error(e) => {
            ops_message::log_error(&e);
            send_fatal_alert(pool, &e).await;
            result.phase1_fatal_error = Some(e);
        }
    }

    result.phase2 = phase2_fetch::run(pool, override_month).await;
    ops_message::log_errors(&result.phase2.errors);

    result.phase3 = phase3_parse_register::run(pool).await;
    ops_message::log_errors(&result.phase3.errors);

    result.phase4 = phase4_store_notify::run(pool).await;
    ops_message::log_errors(&result.phase4.errors);

    tracing::info!("[Pipeline] 完了: {result}");
    result
}

enum Phase1Outcome {
    Success(phase1_watch::Phase1Result),
    LockedSkip(String),
    Error(String),
}

/// Phase1をIMAPサーキットブレイカーで保護しつつ実行する。
///
/// - 既にロック中なら、今回はIMAP接続自体を試みずスキップする
///   （ロック中に再接続を試みること自体がGoogle再ブロックのリスクになるため）。
/// - AUTHENTICATIONFAILEDがLOCK_THRESHOLD回連続したら自動ロックする。
/// - 認証失敗以外のエラー（ネットワーク瞬断等）ではブレイカーを作動させない。
async fn run_phase1_with_breaker(pool: &PgPool) -> Phase1Outcome {
    let mailbox = match imap_util::ImapConfig::load(pool).await {
        Ok(c) => c.mailbox_id().to_string(),
        Err(e) => {
            let kind = ops_message::classify_phase1_fatal(&e);
            let (process, impact) = match kind {
                ops_message::Phase1FatalKind::Database => (
                    "DB読取(自社情報のIMAP認証)",
                    "メール読取不可（認証情報をDBから取れず新着走査全体が停止）"
                ),
                ops_message::Phase1FatalKind::ConfigMissing => (
                    "IMAP認証情報未設定",
                    "メール読取不可（認証情報が設定されていません）"
                ),
                _ => (
                    "IMAP設定読込",
                    "メール読取不可（新着走査全体が停止）"
                )
            };
            return Phase1Outcome::Error(ops_message::fail(
                "メール取込",
                "Phase1",
                process,
                impact,
                e,
            ));
        }
    };

    if let Some(reason) = circuit_breaker::locked_reason(pool, &mailbox).await {
        return Phase1Outcome::LockedSkip(ops_message::fail(
            "メール取込",
            "Phase1",
            "IMAP読取",
            "連続認証失敗のためロック中。管理者が解除するまで新着走査は停止",
            format!("ロック理由: {reason}"),
        ));
    }

    match phase1_watch::run(pool).await {
        Ok(p1) => {
            circuit_breaker::record_success(pool, &mailbox).await;
            Phase1Outcome::Success(p1)
        }
        Err(e) if circuit_breaker::is_auth_failure(&e) => {
            let failures = circuit_breaker::record_failure(pool, &mailbox).await;
            if failures >= circuit_breaker::LOCK_THRESHOLD {
                circuit_breaker::lock(pool, &mailbox, &e).await;
                let msg = format!(
                    "連続{failures}回認証失敗のためIMAPロック。解除まで新着走査は停止"
                );
                Phase1Outcome::Error(ops_message::fail(
                    "メール取込",
                    "Phase1",
                    "IMAP読取(認証)",
                    &msg,
                    e,
                ))
            } else {
                Phase1Outcome::Error(ops_message::fail(
                    "メール取込",
                    "Phase1",
                    "IMAP読取(認証)",
                    "今回の新着メール走査不可（認証失敗）",
                    e,
                ))
            }
        }
        Err(e) => Phase1Outcome::Error(ops_message::fail(
            "メール取込",
            "Phase1",
            "IMAP読取",
            "今回の新着メール走査不可",
            e,
        )),
    }
}

/// Phase1のIMAP接続/認証エラーなど、パイプライン全体が止まる致命的エラーをADMINへ即時通知する。
/// Phase4の「要確認」集約通知（個別ドキュメント単位）とは別枠の即時アラート。
async fn send_fatal_alert(pool: &PgPool, error: &str) {
    let today = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();

    // 原因種別を分類して、件名とアドバイスを切り替える
    let kind = ops_message::classify_phase1_fatal(error);
    let copy = ops_message::fatal_alert_copy(kind);

    // メールでの通知は自社SMTP(=IMAPと同じGmailアカウント認証情報)経由のため、
    // 今回の致命的エラーがまさにその認証情報自体の問題だった場合は送信も失敗し、
    // 管理者に何も届かないまま長時間気づかれない恐れがある。Google Chat Webhookはこの認証情報と無関係な
    // 別チャネルのため、メールと併用して必ず投稿する。
    let chat_text = format!(
        "*【Sophia】{} ({today})*\n{error}\n\n\
         ※これはダッシュボード一覧への書き込み失敗とは別です。\
         {}\n\
         （既に取込済みのメールへの影響はありません）",
        copy.title, copy.advice
    );
    crate::domain::services::chat_notifier::post(&chat_text).await;

    // s_userにrole列は存在しない。ADMIN判定は is_staff=TRUE（role.rs::get_role()と同じ基準）。
    let admin_email_result: Result<Option<(String,)>, sqlx::Error> = sqlx::query_as(
        "SELECT email FROM s_user WHERE is_staff = TRUE AND email != '' LIMIT 1",
    )
    .fetch_optional(pool)
    .await;

    let admin_email = match admin_email_result {
        Ok(Some((email,))) => email,
        Ok(None) => {
            tracing::error!("[Pipeline] ADMINユーザーが見つからず致命的エラーアラートメールを送信できません(Chatへは投稿試行済み)");
            return;
        }
        Err(e) => {
            ops_message::log_error(&ops_message::fail(
                "メール取込",
                "Phase1",
                "致命アラートの宛先読取",
                "管理者メールは送れない（Chatへは投稿試行済み）",
                ops_message::format_sqlx(&e),
            ));
            return;
        }
    };

    let subject = format!("【Sophia】{} ({today})", copy.title);
    let body = format!(
        "管理者各位\n\nメール自動取込のPhase1（新着メール走査）で致命的エラーが発生し、\
         今回の走査は実行できませんでした。\n\
         ※ダッシュボード「メールチェック一覧」への書き込み失敗とは別の障害です。\n\n\
         原因分類: {}\n\n\
         エラー内容:\n{error}\n\n\
         対応:\n{}\n\n\
         （既に取込済みのメールへの影響はありません。IMAPロック中でなければ次回スケジューラ実行時に\
         自動的に再試行されます。ロック中の場合は管理者が手動で解除するまで自動実行されません）\n",
        copy.title, copy.advice
    );

    let email_svc = crate::domain::services::email_service::EmailService::new(pool.clone());
    if let Err(e) = email_svc.send(&admin_email, None, &subject, &body).await {
        tracing::error!("[Pipeline] 致命的エラーアラートメール送信失敗(Chatへは投稿試行済み): {e}");
    } else {
        tracing::info!("[Pipeline] 致命的エラーアラート送信完了 → {admin_email}");
    }
}
