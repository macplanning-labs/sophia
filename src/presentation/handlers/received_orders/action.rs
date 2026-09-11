use axum::{
    extract::{Path, State},
    response::IntoResponse,
    response::Redirect,
};
use sqlx::PgPool;
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
pub async fn rollforward_all(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    match rollforward::rollforward_all_recurring(&pool).await {
        Ok(results) => {
            tracing::info!("一括ロールフォワード: {}件処理", results.len());
        }
        Err(e) => {
            tracing::error!("一括ロールフォワードエラー: {}", e);
        }
    }
    Redirect::to("/received-orders")
}

/// POST /received-orders/{id}/send-report — 報告書メール送付
pub async fn send_report(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::{EmailService, compose_work_report_share_email};

    // 受注情報を取得
    let order = order_repo::find_received_order(&pool, id).await.ok().flatten();

    if let Some(order) = order {
        // クライアント情報を取得（メールアドレス含む）
        let client_info = order_repo::find_client_name_email(&pool, order.client_id).await.ok().flatten();

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
