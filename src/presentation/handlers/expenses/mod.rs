/// presentation/handlers/expenses/ — 経費申請 CRUD + 複数明細 + スマホカメラ連携アップロード
///
/// 機能単位でサブモジュールに分割:
/// - legacy: 旧SSRハンドラ（create/submit/approve/reject。createは未ルーティングの到達不能コード）
/// - api: SPA用 JSON API（ヘッダーCRUD・承認/差戻し・マスタ選択肢）
/// - items: 明細（行）のCRUD JSON API
/// - mobile_upload: スマホからの領収書写真アップロード（トークン発行・アップロード・同期ステータス確認）
///
/// ## エンドポイント
/// - GET  /expenses                                          — 一覧
/// - GET  /expenses/{id}                                     — 詳細
/// - POST /expenses/{id}/submit・approve・reject              — 旧SSR（現在未使用）
/// - GET/POST /api/expenses                                  — 一覧・作成（ヘッダー+初期明細）
/// - GET/PUT/DELETE /api/expenses/{id}                       — 詳細・更新・削除
/// - POST /api/expenses/{id}/approve・reject・unapprove・resubmit — 承認・差戻し・承認取消・再申請
/// - POST /api/expenses/{id}/items                           — 明細追加
/// - PUT/DELETE /api/expenses/items/{item_id}                — 明細更新・削除
/// - GET  /api/expenses/items/{item_id}/receipt              — 領収書画像取得
/// - POST /api/expenses/items/{item_id}/receipt              — PCからの領収書画像アップロード（貼り付け/ファイル選択）
/// - POST /api/expenses/items/{item_id}/mobile-upload/token  — スマホ連携トークン発行（PCセッション認証）
/// - POST /api/mobile-upload/{token}                         — スマホからの画像アップロード（トークン認証・セッション不要）
/// - GET  /api/mobile-upload/{token}/status                  — PC側のアップロード完了ポーリング

mod legacy;
mod api;
mod items;
mod mobile_upload;

pub use legacy::*;
pub use api::*;
pub use items::*;
pub use mobile_upload::*;

/// サマリー（現状フロントでは未使用だが型として残す）
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ExpenseSummary {
    pub total: i64,
    pub draft: i64,
    pub submitted: i64,
    pub approved: i64,
    pub total_amount: i64,
}

/// フィルタ（現状フロントでは未使用だが型として残す）
#[derive(Debug, serde::Deserialize, Default)]
pub struct ExpenseFilter {
    pub status: Option<String>,
}
