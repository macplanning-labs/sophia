/// presentation/handlers/home/ — パイプラインダッシュボード
///
/// 発注進捗・受注進捗のステータスドット表示。
/// EDI_MPのダッシュボードをRustに移植。
///
/// 機能単位でサブモジュールに分割（2026-07-13、P2-3続き）:
/// - dashboard: 進捗集計・期限計算・ステータス変更・ダッシュボードAPI
/// - mail: メールチェック・確認・Webhook受信
/// - edi: EDI-OASIS連携（注文書/請求書取込・承諾・PDF生成）

mod dashboard;
mod mail;
mod mail_briefs;
mod edi;
mod notifications;

pub use dashboard::*;
pub use mail::*;
pub use mail_briefs::*;
pub use edi::*;
pub use notifications::*;

use serde::Serialize;

#[derive(Serialize)]
pub struct StatusResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
