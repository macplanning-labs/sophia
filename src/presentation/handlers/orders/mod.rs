/// presentation/handlers/orders/ — 発注書 CRUD + PDF + メール送信
///
/// Phase 3: 発注管理の中核。
///
/// 機能単位でサブモジュールに分割（2026-07-13、P2-3続き）:
/// - legacy: SSRフォーム時代のハンドラ（create/update/delete等、一部は`-legacy`ルート or dead code）
/// - api: SPA用 JSON API（api_*）
/// - pdf: 発注書PDFデータ構築（orders/ 内および token.rs から共通利用）
///
/// ## エンドポイント
/// - GET  /orders                    — 一覧（フィルタ: partner, status）
/// - GET  /orders/new                — 新規作成フォーム
/// - POST /orders                    — 作成（有効パートナー契約から生成）
/// - GET  /orders/{id}               — 詳細（明細 + ステータス表示）
/// - POST /orders/{id}/rollforward   — 翌月ロールフォワード
/// - POST /orders/{id}/publish       — 発注書PDF添付メール送付
/// - POST /api/orders/{id}/request-timesheet — 稼働報告提出依頼メール
/// - GET  /api/orders/{id}/request-timesheet-preview — 稼働報告提出依頼メール本文プレビュー
/// - GET  /orders/{id}/email-preview — 発注書メール送信プレビュー取得
/// - GET  /orders/{id}/pdf           — 発注書PDFダウンロード
/// - POST /orders/{id}/status        — ステータス更新

mod legacy;
mod api;
mod pdf;

pub use legacy::*;
pub use api::*;
pub use pdf::*;

use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo;

#[derive(Debug, serde::Deserialize)]
pub struct StatusForm {
    pub status: String,
}

/// 発注書番号を自動採番する（PO-YYYYMM-001）
async fn generate_order_id(pool: &PgPool, work_start: chrono::NaiveDate) -> String {
    let prefix = format!("PO-{}", work_start.format("%Y%m"));
    let max_seq = order_repo::find_max_order_id_with_prefix(pool, &format!("{}%", prefix))
        .await
        .ok()
        .flatten();

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    format!("{}-{:03}", prefix, next_seq)
}
