/// routes.rs — ルーター定義
///
/// 全エンドポイントをここで集約する。
///
/// ## ルートグループ × ミドルウェア × アクセス可能ロール
///
/// | グループ         | ミドルウェア      | Admin | Partner | Engineer | Employee | 備考                                        |
/// |:-----------------|:-----------------|:-----:|:-------:|:--------:|:--------:|:--------------------------------------------|
/// | admin_routes     | admin_required   |  ✅   |   ❌    |   ❌     |   ❌     | 給与計算・確定・支払、経費承認、ユーザー管理API、マスタAPI |
/// | employee_routes  | employee_required|  ✅   |   ❌    |   ❌     |   ✅     | 社員共用API（給与は本人分のみ、ハンドラ内でemployee_idを照合） |
/// | portal_routes    | partner_required |  ❌   |   ✅    |   ✅     |   ❌     | パートナーポータル                          |
/// | webhook_router   | Governor(IP)     |  ✅   |   ✅    |   ✅     |   ✅     | Webhook（タイムシート・メール・Peppol、IP段レート制限）|
/// | （認証不要）      | なし             |  ✅   |   ✅    |   ✅     |   ✅     | ログイン・トークン・招待                     |
///
/// ※ 旧 `staff_required` は `admin_required` のエイリアス（role.rs参照）。実質的に同一グループ。
///
/// ## 新しいエンドポイント追加時のチェックリスト
/// 1. 誰がアクセスするか → 上記の表でグループを決定
/// 2. Admin と Partner 両方に開放したい場合 → admin_routes と portal_routes の両方に登録
/// 3. マスタAPI（/api/masters/*）・ユーザー管理API（/api/users/*）は admin_routes 配下のみ
/// 4. 「本人分のみ許可」系（給与など）は employee_routes に置いた上で、ハンドラ内で
///    `AuthUser.employee_id()` と対象データの employee_id が一致するかを個別にチェックする
/// 5. フロントエンドの fetch URL は lib/useMasterData.ts のキー名一覧を参照
/// 6. **新規 API は `/api/v1/` 配下に置く**（開発標準書 §14）。画面パスと同階層に API を生やさない
use axum::{
    http::{header, HeaderValue, Method},
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
    GovernorLayer,
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use crate::config::AppState;
use crate::infrastructure::repositories::attempt_lock_store::InMemoryAttemptStore;
use crate::presentation::handlers;
use crate::presentation::middleware::api_key_auth::{api_key_auth, ApiKeyExtractor};
use crate::presentation::middleware::auth::auth_middleware;
use crate::presentation::middleware::role::{
    admin_required, employee_or_admin_required, partner_required,
};
use auth_core::domain::attempt_lock::{
    AttemptLockConfig, AttemptOutcome, AttemptStore, DefaultAttemptOutcome,
};
use auth_core::infrastructure::rate_limit::attempt_lock_middleware;
use std::sync::Arc;

pub fn create_router(state: AppState) -> Router {
    // ── 社員/管理者ログイン試行制限（Step3 Phase5-3） ──
    // login_guard.rs の login_key 相当をauth-coreの汎用ミドルウェアへ置き換え。
    // /api/auth/login・/api/v1/auth/login にのみ適用するため専用のサブルーターに
    // route_layer で配線する（appチェーン全体に .layer するとリクエストボディを
    // 32KiBまでバッファする処理が全ルートにかかってしまうため、必ずサブルーター経由にする）。
    let attempt_store: Arc<dyn AttemptStore> = Arc::new(InMemoryAttemptStore::new());
    let attempt_outcome: Arc<dyn AttemptOutcome> = Arc::new(DefaultAttemptOutcome);
    let login_routes = Router::new()
        .route("/api/v1/auth/login", post(handlers::auth::api_login))
        .route("/api/auth/login", post(handlers::auth::api_login))
        .route_layer(middleware::from_fn(attempt_lock_middleware::<
            handlers::auth::LoginForm,
        >(
            attempt_store,
            attempt_outcome,
            AttemptLockConfig::default(),
        )));

    // ── 1. Admin専用ルート ──
    let admin_routes = Router::new()
        // 給与管理（計算・確認・支払はAdmin専用）— JSON API
        // ※ 一覧/詳細(GET /api/v1/payroll, /api/v1/payroll/{id})は employee_routes 側で
        //   本人の employee_id と一致する場合のみ閲覧可、というロジックで別途制御しているため
        //   ここには含めない。calculate/confirm/paidは承認・確定系のAdmin操作のため専用ガード。
        .route(
            "/api/v1/payroll/calculate",
            post(handlers::payroll::api_calculate),
        )
        .route(
            "/api/payroll/calculate",
            post(handlers::payroll::api_calculate),
        )
        .route(
            "/api/v1/payroll/{id}/confirm",
            post(handlers::payroll::api_confirm),
        )
        .route(
            "/api/payroll/{id}/confirm",
            post(handlers::payroll::api_confirm),
        )
        .route(
            "/api/v1/payroll/{id}/recalculate-deductions",
            post(handlers::payroll::api_recalculate_deductions),
        )
        .route(
            "/api/payroll/{id}/recalculate-deductions",
            post(handlers::payroll::api_recalculate_deductions),
        )
        .route(
            "/api/v1/payroll/{id}/paid",
            post(handlers::payroll::api_paid),
        )
        .route("/api/payroll/{id}/paid", post(handlers::payroll::api_paid))
        // 経費承認・差戻しは employee_routes の /api/v1/expenses/{id}/approve|reject
        // ── ユーザー管理 JSON API（Admin専用）──
        .route(
            "/api/v1/users",
            get(handlers::users::api_index).post(handlers::users::api_create),
        )
        .route(
            "/api/users",
            get(handlers::users::api_index).post(handlers::users::api_create),
        )
        .route(
            "/api/v1/users/{id}",
            get(handlers::users::api_detail)
                .put(handlers::users::api_update)
                .delete(handlers::users::api_delete),
        )
        .route(
            "/api/users/{id}",
            get(handlers::users::api_detail)
                .put(handlers::users::api_update)
                .delete(handlers::users::api_delete),
        )
        .route(
            "/api/v1/users/{id}/toggle",
            post(handlers::users::api_toggle_active),
        )
        .route(
            "/api/users/{id}/toggle",
            post(handlers::users::api_toggle_active),
        )
        .route(
            "/api/v1/users/{id}/reset-password",
            post(handlers::users::api_reset_password),
        )
        .route(
            "/api/users/{id}/reset-password",
            post(handlers::users::api_reset_password),
        )
        // ── マスタメンテナンス JSON API（Admin専用）──
        .route("/api/v1/masters/meta", get(handlers::masters::api_meta))
        .route("/api/masters/meta", get(handlers::masters::api_meta))
        .route(
            "/api/v1/masters/{table}/fk-options",
            get(handlers::masters::api_fk_options),
        )
        .route(
            "/api/masters/{table}/fk-options",
            get(handlers::masters::api_fk_options),
        )
        .route(
            "/api/v1/masters/{table}",
            get(handlers::masters::api_list).post(handlers::masters::api_create),
        )
        .route(
            "/api/masters/{table}",
            get(handlers::masters::api_list).post(handlers::masters::api_create),
        )
        .route(
            "/api/v1/masters/{table}/{id}",
            get(handlers::masters::api_get)
                .put(handlers::masters::api_update)
                .delete(handlers::masters::api_delete),
        )
        .route(
            "/api/masters/{table}/{id}",
            get(handlers::masters::api_get)
                .put(handlers::masters::api_update)
                .delete(handlers::masters::api_delete),
        )
        // ── 請求書承認・送信ワークフロー（Admin専用）──
        .route(
            "/api/v1/invoices/{id}/email-preview",
            get(handlers::invoices::api_email_preview),
        )
        .route(
            "/api/invoices/{id}/email-preview",
            get(handlers::invoices::api_email_preview),
        )
        .route(
            "/api/v1/invoices/{id}/approve",
            post(handlers::invoices::api_approve),
        )
        .route(
            "/api/invoices/{id}/approve",
            post(handlers::invoices::api_approve),
        )
        .route(
            "/api/v1/invoices/{id}/reject",
            post(handlers::invoices::api_reject),
        )
        .route(
            "/api/invoices/{id}/reject",
            post(handlers::invoices::api_reject),
        )
        .route(
            "/api/v1/invoices/{id}/send",
            post(handlers::invoices::send_mail),
        )
        .route(
            "/api/invoices/{id}/send",
            post(handlers::invoices::send_mail),
        )
        // ── Peppol送信・送受信ログ（Admin専用）──
        .route(
            "/api/v1/invoices/{id}/peppol/send",
            post(handlers::peppol::send_invoice),
        )
        .route(
            "/api/invoices/{id}/peppol/send",
            post(handlers::peppol::send_invoice),
        )
        .route(
            "/api/v1/notices/{id}/peppol/send",
            post(handlers::peppol::send_notice),
        )
        .route(
            "/api/notices/{id}/peppol/send",
            post(handlers::peppol::send_notice),
        )
        .route(
            "/api/v1/peppol/transmissions",
            get(handlers::peppol::list_transmissions),
        )
        .route(
            "/api/peppol/transmissions",
            get(handlers::peppol::list_transmissions),
        )
        // ── 自社情報設定（Admin専用。銀行口座・SMTP認証情報を含むため）──
        .route(
            "/api/v1/company-info",
            get(handlers::company_info::api_get).put(handlers::company_info::api_update),
        )
        .route(
            "/api/company-info",
            get(handlers::company_info::api_get).put(handlers::company_info::api_update),
        )
        .route(
            "/api/v1/company-info/test-email",
            post(handlers::company_info::api_test_email),
        )
        .route(
            "/api/company-info/test-email",
            post(handlers::company_info::api_test_email),
        )
        // ── IMAPサーキットブレイカー（連続認証失敗時の自動ロック解除。Admin専用）──
        .route(
            "/api/v1/mail/imap-lock-status",
            get(handlers::home::imap_lock_status),
        )
        .route(
            "/api/mail/imap-lock-status",
            get(handlers::home::imap_lock_status),
        )
        .route(
            "/api/v1/mail/imap-unlock",
            post(handlers::home::imap_unlock),
        )
        .route("/api/mail/imap-unlock", post(handlers::home::imap_unlock))
        .route("/api/v1/mail/imap-test", post(handlers::home::imap_test))
        .route("/api/mail/imap-test", post(handlers::home::imap_test))
        // ── APIキー管理（Admin専用）──
        .route(
            "/api/v1/settings/api-keys",
            get(handlers::settings::api_keys::list_api_keys)
                .post(handlers::settings::api_keys::generate_api_key),
        )
        .route(
            "/api/v1/settings/api-keys/{id}/revoke",
            post(handlers::settings::api_keys::revoke_api_key),
        )
        // Admin専用ガード
        .layer(middleware::from_fn(admin_required));

    // ── 2. Employee + Admin 共用ルート ──
    let employee_routes = Router::new()
        // ── セキュリティ設定（MFA）— JSON API
        // §14: 正規パスは /api/v1/...。旧パスは移行完了までデュアルマウント。
        .route(
            "/api/v1/settings/security/totp/begin",
            post(handlers::security::totp_begin),
        )
        .route(
            "/api/v1/settings/security/totp/confirm",
            post(handlers::security::totp_confirm),
        )
        .route(
            "/api/v1/settings/security/totp/disable",
            post(handlers::security::totp_disable),
        )
        .route(
            "/api/v1/settings/security/passkey/register/begin",
            post(handlers::security::passkey_register_begin),
        )
        .route(
            "/api/v1/settings/security/passkey/register/complete",
            post(handlers::security::passkey_register_complete),
        )
        .route(
            "/api/v1/settings/security/passkey/{id}/delete",
            post(handlers::security::passkey_delete),
        )
        // ── エンジニア招待 ──
        .route(
            "/api/v1/invite",
            post(handlers::daily_work_entry::invite_engineer),
        )
        .route(
            "/api/invite",
            post(handlers::daily_work_entry::invite_engineer),
        )
        .route(
            "/api/v1/engineers/options",
            get(handlers::daily_work_entry::api_engineer_options),
        )
        .route(
            "/api/engineers/options",
            get(handlers::daily_work_entry::api_engineer_options),
        )
        // ── 発注書アクション（PDF, メール, ステータス — SPAから利用） ──
        .route(
            "/api/v1/orders/{id}/publish-legacy",
            post(handlers::orders::publish),
        )
        .route(
            "/api/orders/{id}/publish-legacy",
            post(handlers::orders::publish),
        )
        .route(
            "/api/v1/orders/{id}/pdf",
            get(handlers::orders::download_pdf),
        )
        .route("/api/orders/{id}/pdf", get(handlers::orders::download_pdf))
        .route(
            "/api/v1/orders/{id}/acceptance-pdf",
            get(handlers::orders::download_acceptance_pdf),
        )
        .route(
            "/api/orders/{id}/acceptance-pdf",
            get(handlers::orders::download_acceptance_pdf),
        )
        .route(
            "/api/v1/orders/{id}/status-legacy",
            post(handlers::orders::update_status),
        )
        .route(
            "/api/orders/{id}/status-legacy",
            post(handlers::orders::update_status),
        )
        .route(
            "/api/v1/orders/{id}/delete-legacy",
            post(handlers::orders::delete),
        )
        .route(
            "/api/orders/{id}/delete-legacy",
            post(handlers::orders::delete),
        )
        // ── ダッシュボード JSON API ──
        .route(
            "/api/v1/orders/{id}/status",
            post(handlers::orders::api_update_status),
        )
        .route(
            "/api/orders/{id}/status",
            post(handlers::orders::api_update_status),
        )
        .route(
            "/api/v1/received-orders/{id}/status",
            post(handlers::home::update_received_order_status),
        )
        .route(
            "/api/received-orders/{id}/status",
            post(handlers::home::update_received_order_status),
        )
        .route(
            "/api/v1/mail/fetch",
            post(handlers::home::manual_mail_fetch),
        )
        .route("/api/mail/fetch", post(handlers::home::manual_mail_fetch))
        .route(
            "/api/v1/mail/{id}/confirm",
            post(handlers::home::confirm_mail),
        )
        .route("/api/mail/{id}/confirm", post(handlers::home::confirm_mail))
        .route(
            "/api/v1/mail/{id}/unconfirm",
            post(handlers::home::unconfirm_mail),
        )
        .route(
            "/api/mail/{id}/unconfirm",
            post(handlers::home::unconfirm_mail),
        )
        .route(
            "/api/v1/mail-briefs",
            get(handlers::home::get_mail_briefs),
        )
        .route("/api/mail-briefs", get(handlers::home::get_mail_briefs))
        .route("/api/v1/edi/import", post(handlers::home::edi_import))
        .route("/api/edi/import", post(handlers::home::edi_import))
        .route(
            "/api/v1/edi/import-all",
            post(handlers::home::edi_import_all),
        )
        .route("/api/edi/import-all", post(handlers::home::edi_import_all))
        .route(
            "/api/v1/edi/import/{email_id}",
            post(handlers::home::edi_import_single),
        )
        .route(
            "/api/edi/import/{email_id}",
            post(handlers::home::edi_import_single),
        )
        .route("/api/v1/edi/orders", get(handlers::home::edi_list_orders))
        .route("/api/edi/orders", get(handlers::home::edi_list_orders))
        .route(
            "/api/v1/edi/orders/{order_id}/import",
            post(handlers::home::edi_import_and_approve),
        )
        .route(
            "/api/edi/orders/{order_id}/import",
            post(handlers::home::edi_import_and_approve),
        )
        .route(
            "/api/v1/edi/invoices",
            get(handlers::home::edi_list_invoices),
        )
        .route("/api/edi/invoices", get(handlers::home::edi_list_invoices))
        .route(
            "/api/v1/edi/invoices/{invoice_id}/approve",
            post(handlers::home::edi_approve_invoice),
        )
        .route(
            "/api/edi/invoices/{invoice_id}/approve",
            post(handlers::home::edi_approve_invoice),
        )
        // ── 支払通知アクション（PDF, メール — SPAから利用） ──
        .route(
            "/api/v1/notices/{id}/email-preview",
            get(handlers::notices::api_email_preview),
        )
        .route(
            "/api/notices/{id}/email-preview",
            get(handlers::notices::api_email_preview),
        )
        .route(
            "/api/v1/notices/{id}/send",
            post(handlers::notices::send_mail),
        )
        .route("/api/notices/{id}/send", post(handlers::notices::send_mail))
        .route(
            "/api/v1/notices/{id}/pdf",
            get(handlers::notices::download_pdf),
        )
        .route(
            "/api/notices/{id}/pdf",
            get(handlers::notices::download_pdf),
        )
        .route(
            "/api/v1/notices/{id}/invoice-pdf",
            get(handlers::notices::download_invoice_pdf),
        )
        .route(
            "/api/notices/{id}/invoice-pdf",
            get(handlers::notices::download_invoice_pdf),
        )
        // ── 受注書アクション ──
        .route(
            "/api/v1/received-orders/{id}/rollforward",
            post(handlers::received_orders::api_rollforward),
        )
        .route(
            "/api/received-orders/{id}/rollforward",
            post(handlers::received_orders::api_rollforward),
        )
        .route(
            "/api/v1/received-orders/{id}/update-status",
            post(handlers::received_orders::update_status),
        )
        .route(
            "/api/received-orders/{id}/update-status",
            post(handlers::received_orders::update_status),
        )
        .route(
            "/api/v1/received-orders/{id}/send-report",
            post(handlers::received_orders::send_report),
        )
        .route(
            "/api/received-orders/{id}/send-report",
            post(handlers::received_orders::send_report),
        )
        .route(
            "/api/v1/received-orders/{id}/delete",
            post(handlers::received_orders::delete),
        )
        .route(
            "/api/received-orders/{id}/delete",
            post(handlers::received_orders::delete),
        )
        .route(
            "/api/v1/received-orders/{id}/link-contract",
            post(handlers::received_orders::api_link_contract),
        )
        .route(
            "/api/received-orders/{id}/link-contract",
            post(handlers::received_orders::api_link_contract),
        )
        .route(
            "/api/v1/received-orders/rollforward-all",
            post(handlers::received_orders::rollforward_all),
        )
        .route(
            "/api/received-orders/rollforward-all",
            post(handlers::received_orders::rollforward_all),
        )
        // ── 請求書アクション（PDF, 削除 — SPAから利用。承認・送信はAdmin専用のためadmin_routes側）──
        .route(
            "/api/v1/invoices/{id}/pdf",
            get(handlers::invoices::download_pdf),
        )
        .route(
            "/api/invoices/{id}/pdf",
            get(handlers::invoices::download_pdf),
        )
        .route(
            "/api/v1/invoices/{id}/delete",
            post(handlers::invoices::delete),
        )
        .route(
            "/api/invoices/{id}/delete",
            post(handlers::invoices::delete),
        )
        // ── 稼働報告アクション ──
        .route(
            "/api/v1/timesheets/upload",
            post(handlers::timesheets::upload),
        )
        .route("/api/timesheets/upload", post(handlers::timesheets::upload))
        .route(
            "/api/v1/timesheets/confirm",
            post(handlers::timesheets::confirm_upload),
        )
        .route(
            "/api/timesheets/confirm",
            post(handlers::timesheets::confirm_upload),
        )
        .route(
            "/api/v1/timesheets/{id}/approve",
            post(handlers::timesheets::approve),
        )
        .route(
            "/api/timesheets/{id}/approve",
            post(handlers::timesheets::approve),
        )
        .route(
            "/api/v1/timesheets/{id}/reject",
            post(handlers::timesheets::reject),
        )
        .route(
            "/api/timesheets/{id}/reject",
            post(handlers::timesheets::reject),
        )
        .route(
            "/api/v1/timesheets/{id}/send",
            post(handlers::timesheets::send_to_client),
        )
        .route(
            "/api/timesheets/{id}/send",
            post(handlers::timesheets::send_to_client),
        )
        // ── タスクアクション ──
        .route("/api/v1/tasks/generate", post(handlers::tasks::generate))
        .route("/api/tasks/generate", post(handlers::tasks::generate))
        .route(
            "/api/v1/tasks/{id}/complete",
            post(handlers::tasks::complete),
        )
        .route("/api/tasks/{id}/complete", post(handlers::tasks::complete))
        .route("/api/v1/tasks/{id}/skip", post(handlers::tasks::skip))
        .route("/api/tasks/{id}/skip", post(handlers::tasks::skip))
        // ── 月次確定 JSON API ──
        .route(
            "/api/v1/settlement",
            get(handlers::settlement_dashboard::api_index),
        )
        .route(
            "/api/settlement",
            get(handlers::settlement_dashboard::api_index),
        )
        .route(
            "/api/v1/settlement/filters",
            get(handlers::settlement_dashboard::api_filters),
        )
        .route(
            "/api/settlement/filters",
            get(handlers::settlement_dashboard::api_filters),
        )
        .route(
            "/api/v1/settlement/issue-invoices",
            post(handlers::settlement_dashboard::api_issue_invoices),
        )
        .route(
            "/api/settlement/issue-invoices",
            post(handlers::settlement_dashboard::api_issue_invoices),
        )
        .route(
            "/api/v1/settlement/issue-notices",
            post(handlers::settlement_dashboard::api_issue_notices),
        )
        .route(
            "/api/settlement/issue-notices",
            post(handlers::settlement_dashboard::api_issue_notices),
        )
        .route(
            "/api/v1/settlement/import-billings",
            post(handlers::settlement_dashboard::api_import_billings),
        )
        .route(
            "/api/settlement/import-billings",
            post(handlers::settlement_dashboard::api_import_billings),
        )
        // ── 一覧 JSON API ──
        .route("/api/v1/auth/me", get(handlers::auth::api_me))
        .route("/api/auth/me", get(handlers::auth::api_me))
        .route("/api/v1/dashboard", get(handlers::home::api_dashboard))
        .route("/api/dashboard", get(handlers::home::api_dashboard))
        .route(
            "/api/v1/dashboard/projects",
            get(handlers::home::api_project_dashboard),
        )
        .route(
            "/api/dashboard/projects",
            get(handlers::home::api_project_dashboard),
        )
        .route(
            "/api/v1/notifications",
            get(handlers::home::api_notifications),
        )
        .route("/api/notifications", get(handlers::home::api_notifications))
        .route(
            "/api/v1/partner-contracts",
            get(handlers::partner_contracts::api_index)
                .post(handlers::partner_contracts::api_create),
        )
        .route(
            "/api/partner-contracts",
            get(handlers::partner_contracts::api_index)
                .post(handlers::partner_contracts::api_create),
        )
        .route(
            "/api/v1/partner-contracts/form-data",
            get(handlers::partner_contracts::api_form_data),
        )
        .route(
            "/api/partner-contracts/form-data",
            get(handlers::partner_contracts::api_form_data),
        )
        .route(
            "/api/v1/orders",
            get(handlers::orders::api_index).post(handlers::orders::api_create),
        )
        .route(
            "/api/orders",
            get(handlers::orders::api_index).post(handlers::orders::api_create),
        )
        .route(
            "/api/v1/orders/form-data",
            get(handlers::orders::api_form_data),
        )
        .route(
            "/api/orders/form-data",
            get(handlers::orders::api_form_data),
        )
        .route("/api/v1/notices", get(handlers::notices::api_index))
        .route("/api/notices", get(handlers::notices::api_index))
        .route(
            "/api/v1/notices/{id}/delete",
            post(handlers::notices::api_delete),
        )
        .route(
            "/api/notices/{id}/delete",
            post(handlers::notices::api_delete),
        )
        .route(
            "/api/v1/client-contracts",
            get(handlers::client_contracts::api_index).post(handlers::client_contracts::api_create),
        )
        .route(
            "/api/client-contracts",
            get(handlers::client_contracts::api_index).post(handlers::client_contracts::api_create),
        )
        .route(
            "/api/v1/client-contracts/form-data",
            get(handlers::client_contracts::api_form_data),
        )
        .route(
            "/api/client-contracts/form-data",
            get(handlers::client_contracts::api_form_data),
        )
        .route("/api/v1/projects", get(handlers::projects::api_index))
        .route("/api/projects", get(handlers::projects::api_index))
        .route(
            "/api/v1/projects/wizard/form-data",
            get(handlers::projects::api_wizard_form_data),
        )
        .route(
            "/api/projects/wizard/form-data",
            get(handlers::projects::api_wizard_form_data),
        )
        .route(
            "/api/v1/projects/wizard",
            post(handlers::projects::api_wizard_create),
        )
        .route(
            "/api/projects/wizard",
            post(handlers::projects::api_wizard_create),
        )
        .route(
            "/api/v1/received-orders",
            get(handlers::received_orders::api_index).post(handlers::received_orders::api_create),
        )
        .route(
            "/api/received-orders",
            get(handlers::received_orders::api_index).post(handlers::received_orders::api_create),
        )
        .route("/api/v1/invoices", get(handlers::invoices::api_index))
        .route("/api/invoices", get(handlers::invoices::api_index))
        .route("/api/v1/timesheets", get(handlers::timesheets::api_index))
        .route("/api/timesheets", get(handlers::timesheets::api_index))
        .route("/api/v1/tasks", get(handlers::tasks::api_index))
        .route("/api/tasks", get(handlers::tasks::api_index))
        .route(
            "/api/v1/employees",
            get(handlers::employees::api_index).post(handlers::employees::api_create),
        )
        .route(
            "/api/employees",
            get(handlers::employees::api_index).post(handlers::employees::api_create),
        )
        .route(
            "/api/v1/expenses",
            get(handlers::expenses::api_index).post(handlers::expenses::api_create),
        )
        .route(
            "/api/expenses",
            get(handlers::expenses::api_index).post(handlers::expenses::api_create),
        )
        .route(
            "/api/v1/expenses/categories",
            get(handlers::expenses::api_category_options),
        )
        .route(
            "/api/expenses/categories",
            get(handlers::expenses::api_category_options),
        )
        .route(
            "/api/v1/employees/options",
            get(handlers::expenses::api_employee_options),
        )
        .route(
            "/api/employees/options",
            get(handlers::expenses::api_employee_options),
        )
        .route("/api/v1/payroll", get(handlers::payroll::api_index))
        .route("/api/payroll", get(handlers::payroll::api_index))
        .route("/api/v1/security", get(handlers::security::api_index))
        .route("/api/security", get(handlers::security::api_index))
        // ── 詳細 JSON API ──
        .route(
            "/api/v1/partner-contracts/{id}",
            get(handlers::partner_contracts::api_detail)
                .put(handlers::partner_contracts::api_update)
                .delete(handlers::partner_contracts::api_delete),
        )
        .route(
            "/api/partner-contracts/{id}",
            get(handlers::partner_contracts::api_detail)
                .put(handlers::partner_contracts::api_update)
                .delete(handlers::partner_contracts::api_delete),
        )
        .route(
            "/api/v1/partner-contracts/{id}/extend",
            post(handlers::partner_contracts::api_extend),
        )
        .route(
            "/api/partner-contracts/{id}/extend",
            post(handlers::partner_contracts::api_extend),
        )
        .route(
            "/api/v1/orders/{id}",
            get(handlers::orders::api_detail)
                .put(handlers::orders::api_update)
                .delete(handlers::orders::api_delete),
        )
        .route(
            "/api/orders/{id}",
            get(handlers::orders::api_detail)
                .put(handlers::orders::api_update)
                .delete(handlers::orders::api_delete),
        )
        .route(
            "/api/v1/orders/{id}/rollforward",
            post(handlers::orders::api_rollforward),
        )
        .route(
            "/api/orders/{id}/rollforward",
            post(handlers::orders::api_rollforward),
        )
        .route(
            "/api/v1/orders/{id}/publish",
            post(handlers::orders::api_publish),
        )
        .route(
            "/api/orders/{id}/publish",
            post(handlers::orders::api_publish),
        )
        .route(
            "/api/v1/orders/{id}/republish",
            post(handlers::orders::api_republish),
        )
        .route(
            "/api/orders/{id}/republish",
            post(handlers::orders::api_republish),
        )
        .route(
            "/api/v1/orders/{id}/request-timesheet",
            post(handlers::orders::api_request_timesheet),
        )
        .route(
            "/api/orders/{id}/request-timesheet",
            post(handlers::orders::api_request_timesheet),
        )
        .route(
            "/api/v1/orders/{id}/request-timesheet-preview",
            get(handlers::orders::api_request_timesheet_preview),
        )
        .route(
            "/api/orders/{id}/request-timesheet-preview",
            get(handlers::orders::api_request_timesheet_preview),
        )
        .route(
            "/api/v1/orders/{id}/email-preview",
            get(handlers::orders::api_email_preview),
        )
        .route(
            "/api/orders/{id}/email-preview",
            get(handlers::orders::api_email_preview),
        )
        .route(
            "/api/v1/notices/{id}",
            get(handlers::notices::api_detail).put(handlers::notices::api_update),
        )
        .route(
            "/api/notices/{id}",
            get(handlers::notices::api_detail).put(handlers::notices::api_update),
        )
        .route(
            "/api/v1/client-contracts/{id}",
            get(handlers::client_contracts::api_detail)
                .put(handlers::client_contracts::api_update)
                .delete(handlers::client_contracts::api_delete),
        )
        .route(
            "/api/client-contracts/{id}",
            get(handlers::client_contracts::api_detail)
                .put(handlers::client_contracts::api_update)
                .delete(handlers::client_contracts::api_delete),
        )
        .route(
            "/api/v1/client-contracts/{id}/extend",
            post(handlers::client_contracts::api_extend),
        )
        .route(
            "/api/client-contracts/{id}/extend",
            post(handlers::client_contracts::api_extend),
        )
        .route(
            "/api/v1/projects/{project_id}",
            get(handlers::projects::api_detail)
                .put(handlers::projects::api_update)
                .delete(handlers::projects::api_delete),
        )
        .route(
            "/api/projects/{project_id}",
            get(handlers::projects::api_detail)
                .put(handlers::projects::api_update)
                .delete(handlers::projects::api_delete),
        )
        .route(
            "/api/v1/received-orders/{id}",
            get(handlers::received_orders::api_detail)
                .put(handlers::received_orders::api_update)
                .delete(handlers::received_orders::api_delete),
        )
        .route(
            "/api/received-orders/{id}",
            get(handlers::received_orders::api_detail)
                .put(handlers::received_orders::api_update)
                .delete(handlers::received_orders::api_delete),
        )
        .route(
            "/api/v1/invoices/{id}",
            get(handlers::invoices::api_detail).put(handlers::invoices::api_update),
        )
        .route(
            "/api/invoices/{id}",
            get(handlers::invoices::api_detail).put(handlers::invoices::api_update),
        )
        .route(
            "/api/v1/timesheets/{id}",
            get(handlers::timesheets::api_detail),
        )
        .route(
            "/api/timesheets/{id}",
            get(handlers::timesheets::api_detail),
        )
        .route("/api/v1/payroll/{id}", get(handlers::payroll::api_detail))
        .route("/api/payroll/{id}", get(handlers::payroll::api_detail))
        .route(
            "/api/v1/employees/{id}",
            get(handlers::employees::api_detail)
                .put(handlers::employees::api_update)
                .delete(handlers::employees::api_delete),
        )
        .route(
            "/api/employees/{id}",
            get(handlers::employees::api_detail)
                .put(handlers::employees::api_update)
                .delete(handlers::employees::api_delete),
        )
        .route(
            "/api/v1/expenses/{id}",
            get(handlers::expenses::api_detail)
                .put(handlers::expenses::api_update)
                .delete(handlers::expenses::api_delete),
        )
        .route(
            "/api/expenses/{id}",
            get(handlers::expenses::api_detail)
                .put(handlers::expenses::api_update)
                .delete(handlers::expenses::api_delete),
        )
        .route(
            "/api/v1/expenses/{id}/approve",
            post(handlers::expenses::api_approve),
        )
        .route(
            "/api/expenses/{id}/approve",
            post(handlers::expenses::api_approve),
        )
        .route(
            "/api/v1/expenses/{id}/reject",
            post(handlers::expenses::api_reject),
        )
        .route(
            "/api/expenses/{id}/reject",
            post(handlers::expenses::api_reject),
        )
        .route(
            "/api/v1/expenses/{id}/unapprove",
            post(handlers::expenses::api_unapprove),
        )
        .route(
            "/api/expenses/{id}/unapprove",
            post(handlers::expenses::api_unapprove),
        )
        .route(
            "/api/v1/expenses/{id}/resubmit",
            post(handlers::expenses::api_resubmit),
        )
        .route(
            "/api/expenses/{id}/resubmit",
            post(handlers::expenses::api_resubmit),
        )
        .route(
            "/api/v1/expenses/{id}/items",
            post(handlers::expenses::api_add_item),
        )
        .route(
            "/api/expenses/{id}/items",
            post(handlers::expenses::api_add_item),
        )
        .route(
            "/api/v1/expenses/items/{item_id}",
            put(handlers::expenses::api_update_item).delete(handlers::expenses::api_delete_item),
        )
        .route(
            "/api/expenses/items/{item_id}",
            put(handlers::expenses::api_update_item).delete(handlers::expenses::api_delete_item),
        )
        .route(
            "/api/v1/expenses/items/{item_id}/receipt",
            get(handlers::expenses::api_item_receipt)
                .post(handlers::expenses::api_upload_item_receipt),
        )
        .route(
            "/api/expenses/items/{item_id}/receipt",
            get(handlers::expenses::api_item_receipt)
                .post(handlers::expenses::api_upload_item_receipt),
        )
        .route(
            "/api/v1/expenses/items/{item_id}/mobile-upload/token",
            post(handlers::expenses::api_issue_mobile_token),
        )
        .route(
            "/api/expenses/items/{item_id}/mobile-upload/token",
            post(handlers::expenses::api_issue_mobile_token),
        )
        // ── 受信メール JSON API ──
        .route(
            "/api/v1/received-emails",
            get(handlers::received_emails::api_index),
        )
        .route(
            "/api/received-emails",
            get(handlers::received_emails::api_index),
        )
        .route(
            "/api/v1/received-emails/{id}/import",
            post(handlers::received_emails::api_import),
        )
        .route(
            "/api/received-emails/{id}/import",
            post(handlers::received_emails::api_import),
        )
        .route(
            "/api/v1/received-emails/{id}/resolve",
            post(handlers::received_emails::api_resolve),
        )
        .route(
            "/api/received-emails/{id}/resolve",
            post(handlers::received_emails::api_resolve),
        )
        .route(
            "/api/v1/received-emails/{id}",
            delete(handlers::received_emails::api_delete),
        )
        .route(
            "/api/received-emails/{id}",
            delete(handlers::received_emails::api_delete),
        )
        // ── 稼働報告 自己申告（案件に紐づかない社員向け、2026-07-30追加）──
        .route(
            "/api/v1/timesheets/self-report",
            get(handlers::timesheet_self_report::api_get)
                .post(handlers::timesheet_self_report::api_submit),
        )
        .route(
            "/api/v1/timesheets/self-report/sheets/prepare",
            post(handlers::timesheet_self_report::api_sheets_prepare),
        )
        .route(
            "/api/v1/timesheets/self-report/sheets/submit",
            post(handlers::timesheet_self_report::api_sheets_submit),
        )
        // Employee以上ガード
        .layer(middleware::from_fn(employee_or_admin_required));

    // ── 3. Partner専用ルート ──
    let portal_routes = Router::new()
        .route(
            "/api/v1/portal/dashboard",
            get(handlers::partner_portal::api_dashboard),
        )
        .route(
            "/api/portal/dashboard",
            get(handlers::partner_portal::api_dashboard),
        )
        .route(
            "/api/v1/portal/orders",
            get(handlers::partner_portal::api_orders),
        )
        .route(
            "/api/portal/orders",
            get(handlers::partner_portal::api_orders),
        )
        .route(
            "/api/v1/portal/orders/{order_id}/approve",
            post(handlers::partner_portal::api_order_approve),
        )
        .route(
            "/api/portal/orders/{order_id}/approve",
            post(handlers::partner_portal::api_order_approve),
        )
        .route(
            "/api/v1/portal/orders/{order_id}/pdf",
            get(handlers::partner_portal::api_order_pdf),
        )
        .route(
            "/api/portal/orders/{order_id}/pdf",
            get(handlers::partner_portal::api_order_pdf),
        )
        .route(
            "/api/v1/portal/orders/{order_id}/acceptance-pdf",
            get(handlers::partner_portal::api_acceptance_pdf),
        )
        .route(
            "/api/portal/orders/{order_id}/acceptance-pdf",
            get(handlers::partner_portal::api_acceptance_pdf),
        )
        // 請求書（支払通知書）
        .route(
            "/api/v1/portal/notices",
            get(handlers::partner_portal::api_notices),
        )
        .route(
            "/api/portal/notices",
            get(handlers::partner_portal::api_notices),
        )
        .route(
            "/api/v1/portal/notices/{notice_id}/confirm",
            post(handlers::partner_portal::api_notice_confirm),
        )
        .route(
            "/api/portal/notices/{notice_id}/confirm",
            post(handlers::partner_portal::api_notice_confirm),
        )
        .route(
            "/api/v1/portal/notices/{notice_id}/invoice-pdf",
            get(handlers::partner_portal::api_invoice_pdf),
        )
        .route(
            "/api/portal/notices/{notice_id}/invoice-pdf",
            get(handlers::partner_portal::api_invoice_pdf),
        )
        .route(
            "/api/v1/portal/notices/{notice_id}/payment-notice-pdf",
            get(handlers::partner_portal::api_payment_notice_pdf),
        )
        .route(
            "/api/portal/notices/{notice_id}/payment-notice-pdf",
            get(handlers::partner_portal::api_payment_notice_pdf),
        )
        // 稼働報告（2ステップ: upload → confirm）
        .route(
            "/api/v1/portal/timesheets",
            get(handlers::partner_timesheet::api_timesheet_list),
        )
        .route(
            "/api/portal/timesheets",
            get(handlers::partner_timesheet::api_timesheet_list),
        )
        .route(
            "/api/v1/portal/timesheets/upload",
            post(handlers::partner_portal::api_timesheet_upload),
        )
        .route(
            "/api/portal/timesheets/upload",
            post(handlers::partner_portal::api_timesheet_upload),
        )
        .route(
            "/api/v1/portal/timesheets/confirm",
            post(handlers::partner_portal::api_timesheet_confirm),
        )
        .route(
            "/api/portal/timesheets/confirm",
            post(handlers::partner_portal::api_timesheet_confirm),
        )
        .route(
            "/api/v1/portal/timesheets/{pk}/submit",
            post(handlers::partner_timesheet::api_submit_timesheet),
        )
        .route(
            "/api/portal/timesheets/{pk}/submit",
            post(handlers::partner_timesheet::api_submit_timesheet),
        )
        // 日次稼働報告（タイムシート入力）
        .route(
            "/api/v1/portal/engineers",
            get(handlers::daily_work_entry::list_engineers),
        )
        .route(
            "/api/portal/engineers",
            get(handlers::daily_work_entry::list_engineers),
        )
        .route(
            "/api/v1/portal/invite",
            post(handlers::daily_work_entry::invite_engineer),
        )
        .route(
            "/api/portal/invite",
            post(handlers::daily_work_entry::invite_engineer),
        )
        .route(
            "/api/v1/portal/work-entries",
            get(handlers::daily_work_entry::list_work_entries)
                .post(handlers::daily_work_entry::save_work_entries),
        )
        .route(
            "/api/portal/work-entries",
            get(handlers::daily_work_entry::list_work_entries)
                .post(handlers::daily_work_entry::save_work_entries),
        )
        .route(
            "/api/v1/portal/work-entries/summary",
            get(handlers::daily_work_entry::work_entries_summary),
        )
        .route(
            "/api/portal/work-entries/summary",
            get(handlers::daily_work_entry::work_entries_summary),
        )
        // PARTNER専用ガード
        .layer(middleware::from_fn(partner_required));

    // ── Webhook系 IP単位レート制限（tower_governor）──
    // タイムシート・メール・Peppol の外部Webhook に IP段のトークンバケット制限を適用。
    // 認証（HMAC/シークレット）とは別の層で、署名検証前の連打（DoS的）を抑止する。
    // `mail_webhook` を `employee_routes` から外した理由：
    //   - ロールガード（employee_or_admin_required）下では未認証の外部GASが
    //     401 Unauthorized またはリダイレクトになり、正常に処理できなくなるため、
    //     認証なしで到達可能な webhook_router に集約する。
    // SmartIpKeyExtractor を使う理由は /v1/ IP段と同じ（nginx背後でX-Forwarded-For参照）。
    let webhook_ip_governor_conf = GovernorConfigBuilder::default()
        .per_second(1) // 60 req/min相当（/v1/ IP段と同値）
        .burst_size(60)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .unwrap();

    let webhook_router = Router::new()
        .route("/api/v1/webhook/timesheet", post(handlers::api::index))
        .route("/api/webhook/timesheet", post(handlers::api::index))
        .route("/api/v1/mail/webhook", post(handlers::home::mail_webhook))
        .route("/api/mail/webhook", post(handlers::home::mail_webhook))
        .route("/api/v1/peppol/inbound", post(handlers::peppol::inbound))
        .route("/api/peppol/inbound", post(handlers::peppol::inbound))
        .layer(GovernorLayer::new(webhook_ip_governor_conf))
        .with_state(state.clone());

    // ── 外部向け WebAPI (/v1/invoices, /v1/orders など) ──
    // レート制限設定（tower-governor）
    // nginx 経由では PeerIp はプロキシIPになるため、X-Forwarded-For 等を参照する SmartIp を使う。
    // 直接接続時は ConnectInfo にフォールバックする。
    let ip_governor_conf = GovernorConfigBuilder::default()
        .per_second(1) // 60 req/min相当
        .burst_size(60)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
        .unwrap();

    let key_governor_conf = GovernorConfigBuilder::default()
        .per_second(5) // 300 req/min相当の目安
        .burst_size(300)
        .key_extractor(ApiKeyExtractor)
        .finish()
        .unwrap();

    let v1_router = Router::new()
        .route(
            "/v1/invoices",
            get(handlers::webapi::invoices::list_invoices),
        )
        .route(
            "/v1/invoices/{id}",
            get(handlers::webapi::invoices::get_invoice),
        )
        .route(
            "/v1/invoices/{id}/accept",
            post(handlers::webapi::invoices::accept_invoice),
        )
        .route("/v1/orders", post(handlers::webapi::orders::submit_order))
        .layer(GovernorLayer::new(key_governor_conf))
        .layer(middleware::from_fn_with_state(state.clone(), api_key_auth))
        .layer(GovernorLayer::new(ip_governor_conf))
        .with_state(state.clone());

    // ── Swagger UI（ENABLE_SWAGGER_UI でゲート） ──
    let swagger_routes = if std::env::var("ENABLE_SWAGGER_UI").map(|v| v.to_lowercase() == "true").unwrap_or(false) {
        tracing::info!("✅ Swagger UI enabled on /api/v1/docs");
        Router::new()
            .route("/api/v1/docs", get(handlers::swagger::swagger_ui))
            .route("/api/v1/docs/openapi.json", get(handlers::swagger::openapi_json))
    } else {
        Router::new()
    };

    // Tauriデスクトップアプリ（macOS/Linux: tauri://localhost, Windows: http://tauri.localhost）から
    // 本番APIを利用可能にするためのCORS許可
    let desktop_cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list([
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
        ]))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    // ── 全体ルーター組み立て ──
    let mut app = Router::new()
        .merge(admin_routes)
        .merge(employee_routes)
        .merge(portal_routes)
        .merge(v1_router)
        .merge(webhook_router)
        .merge(login_routes)
        .merge(swagger_routes)
        // 認証（SSR互換 — 段階的廃止予定）
        // ※ GET/POST /login は登録しない。本番/ステージングはnginxがNext.jsへ転送するため
        //   このRustルートには元々到達せず、ローカル開発(SPA一体配信)でだけ静的ファイル
        //   フォールバックより先にマッチしてパスキー等を含む本来のログインページを隠して
        //   しまっていた。登録しないことでローカルもSPAの/loginがそのまま使われる。
        .route("/logout", get(handlers::auth::logout))
        .route("/health", get(health_check))
        // SPA用 認証JSON API（認証不要）
        // ※ /api/v1/auth/login・/api/auth/login は login_routes（試行制限ミドルウェア付き）で登録済み
        .route("/api/v1/auth/logout", post(handlers::auth::api_logout))
        .route("/api/auth/logout", post(handlers::auth::api_logout))
        .route(
            "/api/v1/auth/portal-login",
            post(handlers::auth::api_portal_login),
        )
        .route(
            "/api/auth/portal-login",
            post(handlers::auth::api_portal_login),
        )
        // パスキーログイン（認証不要）— §14: /api/v1/auth/... が正規
        .route(
            "/api/v1/auth/passkey/login/begin",
            post(handlers::auth::passkey_login_begin),
        )
        .route(
            "/api/v1/auth/passkey/login/complete",
            post(handlers::auth::passkey_login_complete),
        )
        // MFA 検証（ログイン後の2段階目）
        .route("/api/v1/mfa/verify", post(handlers::auth::mfa_verify))
        .route(
            "/api/v1/mfa/passkey/begin",
            post(handlers::auth::passkey_auth_begin),
        )
        .route(
            "/api/v1/mfa/passkey/complete",
            post(handlers::auth::passkey_auth_complete),
        )
        // ポータル魔法リンク確認（§14 JSON）
        .route(
            "/api/v1/auth/portal-link/{token}",
            get(handlers::auth::api_portal_link_status),
        )
        .route(
            "/api/v1/auth/portal-link/{token}/confirm",
            post(handlers::auth::api_portal_link_confirm),
        )
        // エンジニア招待受諾（§14 JSON）
        .route(
            "/api/v1/invite/{uuid}/status",
            get(handlers::invite::api_invite_status),
        )
        .route(
            "/api/v1/invite/{uuid}/accept",
            post(handlers::invite::api_invite_accept),
        )
        .route(
            "/api/v1/invite/{uuid}/register/begin",
            post(handlers::invite::passkey_register_begin),
        )
        .route(
            "/api/v1/invite/{uuid}/register/complete",
            post(handlers::invite::passkey_register_complete),
        )
        // パートナートークン（§14 JSON + PDF）
        .route("/api/v1/token/{uuid}", get(handlers::token::api_view))
        .route(
            "/api/v1/token/{uuid}/accept",
            post(handlers::token::api_accept),
        )
        .route(
            "/api/v1/token/{uuid}/pdf",
            get(handlers::token::download_pdf),
        )
        .route(
            "/api/v1/token/{uuid}/invoice-pdf",
            get(handlers::token::download_invoice_pdf),
        )
        .route(
            "/api/v1/token/{uuid}/acceptance-pdf",
            get(handlers::token::download_acceptance_pdf),
        )
        .route(
            "/api/v1/token/{uuid}/timesheet",
            post(handlers::token::api_upload_timesheet),
        )
        // 旧 PDF 直リンク互換（--spa 一体配信時。nginx 環境は Next redirects → /api/v1/...）
        .route("/token/{uuid}/pdf", get(handlers::token::download_pdf))
        .route(
            "/token/{uuid}/invoice-pdf",
            get(handlers::token::download_invoice_pdf),
        )
        .route(
            "/token/{uuid}/acceptance-pdf",
            get(handlers::token::download_acceptance_pdf),
        )
        // スマホカメラ連携アップロード（認証不要 — トークン自体が期限付き使い切りの認証情報）
        .route(
            "/api/v1/mobile-upload/{token}",
            post(handlers::expenses::api_mobile_upload),
        )
        .route(
            "/api/mobile-upload/{token}",
            post(handlers::expenses::api_mobile_upload),
        )
        .route(
            "/api/v1/mobile-upload/{token}/status",
            get(handlers::expenses::api_mobile_upload_status),
        )
        .route(
            "/api/mobile-upload/{token}/status",
            get(handlers::expenses::api_mobile_upload_status),
        )
        // 認証ミドルウェア
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        // CSRF対策: Origin/Refererチェック（Step3 Phase7-2）。SameSite=Laxのみに
        // 頼らない多層防御。認証ミドルウェアより外側（先に評価）に置き、許可されない
        // Originからの状態変更リクエストは認証チェックの前に弾く。除外パスは
        // sophia_origin_check_middleware内部で判定（Phase7-1でユーザー承認済み）。
        .layer(axum::middleware::from_fn(
            crate::presentation::middleware::origin_check::sophia_origin_check_middleware(
                std::sync::Arc::new(vec![
                    std::env::var("BASE_URL").unwrap_or_default(),
                    // ドメイン移行中の一時許可（sophia.example.com →
                    // sophia-app.example.com）。移行完了後、BASE_URL切替と
                    // 旧ドメイン退役が済んだら、この行と下のtauriエントリの並び順も
                    // 含めて整理を検討する。
                    "https://sophia-app.example.com".to_string(),
                    "tauri://localhost".to_string(),
                    "http://tauri.localhost".to_string(),
                ]),
            ),
        ))
        // 静的ファイル
        .nest_service("/static", ServeDir::new("static"))
        .layer(desktop_cors)
        // 共有ステート
        .with_state(state);

    // SPA 静的ファイル配信 (frontend/out/)
    // 1. ファイルが存在すればそのまま返す
    // 2. 動的ルート（/users/123 等）→ /users/_/index.html にフォールバック
    // 3. それ以外 → index.html にフォールバック
    let spa_dir = std::path::Path::new("frontend/out");
    if spa_dir.exists() {
        tracing::info!("SPA mode: serving frontend/out/ (dynamic route fallback enabled)");
        let serve_dir = ServeDir::new("frontend/out").not_found_service(tower::service_fn(
            |req: axum::http::Request<axum::body::Body>| async move {
                let path = req.uri().path();

                // 動的ルートのフォールバック: /xxx/123 → /xxx/_/index.html
                if let Some(parent) = std::path::Path::new(path).parent() {
                    let fallback = format!("frontend/out{}/_/index.html", parent.to_string_lossy());
                    if std::path::Path::new(&fallback).exists() {
                        let body = tokio::fs::read(&fallback).await.unwrap_or_default();
                        return Ok::<_, std::convert::Infallible>(
                            axum::response::Response::builder()
                                .status(200)
                                .header("content-type", "text/html; charset=utf-8")
                                .body(axum::body::Body::from(body))
                                .unwrap(),
                        );
                    }
                }

                // デフォルトフォールバック: index.html
                let body = tokio::fs::read("frontend/out/index.html")
                    .await
                    .unwrap_or_default();
                Ok(axum::response::Response::builder()
                    .status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(axum::body::Body::from(body))
                    .unwrap())
            },
        ));
        app = app.fallback_service(serve_dir);
    } else {
        tracing::info!("SPA directory not found: frontend/out/ — skipping SPA fallback");
    }

    app
}

async fn health_check() -> &'static str {
    "OK"
}
