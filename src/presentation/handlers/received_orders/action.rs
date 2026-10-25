use axum::{
    extract::{Path, State},
    response::IntoResponse,
    response::Redirect,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;
use crate::domain::authz::stamp::SKIPPED_NO_OWNER_NOTE;
use crate::domain::services::rollforward;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::api_response::AppError;

/// POST /received-orders/{id}/rollforward — 翌月ロールフォワード
pub async fn rollforward_handler(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match rollforward::rollforward_order(&pool, id).await {
        Ok(new_id) => {
            tracing::info!("ロールフォワード完了: {} → {}", id, new_id);
            Redirect::to(&format!("/received-orders/{}", new_id))
        }
        Err(e) => {
            tracing::warn!("ロールフォワード失敗: {}", e);
            Redirect::to(&format!("/received-orders/{}", id))
        }
    }
}

/// POST /received-orders/rollforward-all — 一括ロールフォワード
///
/// 担当が決まらない受注は作らずに続け、件数と案内を返す(`skipped_no_owner`)。
pub async fn rollforward_all(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
) -> axum::response::Response {
    // 担当スタンプを持たない操作(一覧・選択肢・新規作成)。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::OrderReceivedEdit) {
        return crate::presentation::http_util::forbidden();
    }
    match rollforward::rollforward_all_recurring(&pool).await {
        Ok(results) => {
            tracing::info!("一括ロールフォワード: {}件処理, 担当未決定でスキップ{}件", results.created.len(), results.skipped_no_owner);
            let mut body = serde_json::json!({
                "success": true,
                "created": results.created.len(),
                "skipped_no_owner": results.skipped_no_owner,
            });
            if results.skipped_no_owner > 0 {
                body["message"] = serde_json::json!(SKIPPED_NO_OWNER_NOTE);
            }
            return axum::Json(body).into_response();
        }
        Err(e) => {
            tracing::error!("一括ロールフォワードエラー: {}", e);
        }
    }
    Redirect::to("/received-orders").into_response()
}

/// POST /received-orders/{id}/send-report — 報告書メール送付
pub async fn send_report(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    // 入口の権限表に頼らず、ハンドラ自身で権限と担当範囲を判定する(DEMO-000254)
    let stamp = match crate::infrastructure::repositories::stamp_repo::of_received_order(&pool, id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("担当スタンプの取得に失敗: {e:#}");
            return crate::presentation::http_util::forbidden();
        }
    };
    if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::OrderReceivedEdit, &stamp) {
        return crate::presentation::http_util::forbidden();
    }
    send_report_inner(State(pool), Path(id)).await.into_response()
}

async fn send_report_inner(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::{EmailService, compose_work_report_share_email};

    // 受注情報を取得
    let order = order_repo::find_received_order(&pool, id).await.log_err().ok().flatten();

    if let Some(order) = order {
        // クライアント情報を取得（メールアドレス含む）
        let client_info = order_repo::find_client_name_email(&pool, order.client_id).await.log_err().ok().flatten();

        if let Some((client_name, client_email)) = client_info {
            if !client_email.is_empty() {
                // メール送信。失敗した場合はステータスをREPORT_SENTにせず処理を打ち切る
                // （未送信のまま送付済み扱いになるのを防ぐ）
                let year_month = order.target_month.format("%Y年%m月").to_string();
                let ctx = compose_work_report_share_email(
                    &client_name,
                    &year_month,
                    &order.project_name,
                );
                let email_svc = EmailService::new(pool.clone());
                email_svc.send_by_template(
                    "report_send",
                    &client_email,
                    None,
                    &ctx,
                ).await?;
            }
        }
    }

    // ステータスを REPORT_SENT に更新
    order_repo::mark_received_order_report_sent(&pool, id).await?;

    Ok(Redirect::to(&format!("/received-orders/{}", id)))
}
