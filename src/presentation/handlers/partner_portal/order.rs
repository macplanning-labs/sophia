use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
};
use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::middleware::role::AuthUser;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ダッシュボード
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// GET /api/portal/dashboard
pub async fn api_dashboard(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown").to_string();
    let order_count = order_repo::count_orders_for_partner(&pool, &partner_id).await.unwrap_or(0);
    let timesheet_count = order_repo::count_timesheets_for_partner(&pool, &partner_id).await.unwrap_or(0);
    axum::Json(serde_json::json!({ "partner_id": partner_id, "order_count": order_count, "timesheet_count": timesheet_count }))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 注文書
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// GET /api/portal/orders — 注文書一覧
///
/// カラム: order_id, project_name, engineer_name, target_month(work_start), order_date, 金額合計, status, uuid
pub async fn api_orders(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown");

    let rows = order_repo::list_portal_orders(&pool, partner_id).await.unwrap_or_default();

    let orders: Vec<serde_json::Value> = rows.into_iter().map(|(order_id, project_name, engineer_name, work_start, order_date, total_amount, status, uuid)| {
        serde_json::json!({
            "order_id": order_id,
            "project_name": project_name,
            "engineer_name": engineer_name,
            "target_month": work_start.format("%Y年%m月").to_string(),
            "order_date": order_date.format("%Y/%m/%d").to_string(),
            "total_amount": total_amount.unwrap_or(0),
            "status": status,
            "uuid": uuid.to_string(),
        })
    }).collect();

    axum::Json(serde_json::json!({ "orders": orders }))
}

/// POST /api/portal/orders/{order_id}/approve — 注文書承諾
///
/// 処理: ステータス更新 → 注文請書PDF永続保存 → SHA256ハッシュ → 社内通知メール
pub async fn api_order_approve(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(order_id): Path<String>,
) -> impl IntoResponse {
    use sha2::{Sha256, Digest};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let partner_id = auth_user.partner_id().unwrap_or("unknown");

    // 権限チェック + ステータス確認
    let order = order_repo::find_purchase_order_for_partner(&pool, &order_id, partner_id).await;

    let order = match order {
        Ok(Some(o)) => o,
        _ => return axum::Json(serde_json::json!({ "status": "error", "error": "注文書が見つかりません" })),
    };

    if order.status != "SENT" {
        return axum::Json(serde_json::json!({ "status": "error", "error": format!("この注文書は承諾できません（現在のステータス: {}）", order.status) }));
    }

    // 注文請書PDFを生成 → ハッシュ計算
    let pdf_bytes = match crate::presentation::handlers::orders::build_purchase_order_pdf_data(&pool, &order).await {
        Ok(pdf_data) => {
            let gen = PdfGenerator::new();
            match gen.generate_acceptance_pdf(&pdf_data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::error!("注文請書PDF生成エラー: {:?}", e);
                    return axum::Json(serde_json::json!({ "status": "error", "error": "PDF生成に失敗しました" }));
                }
            }
        }
        Err(e) => {
            tracing::error!("PDFデータ構築エラー: {:?}", e);
            return axum::Json(serde_json::json!({ "status": "error", "error": "PDFデータ構築に失敗しました" }));
        }
    };

    // SHA256ハッシュ（改ざん防止 — 電子帳簿保存法の真実性要件）
    let mut hasher = Sha256::new();
    hasher.update(&pdf_bytes);
    let document_hash = format!("{:x}", hasher.finalize());

    // PDF永続保存パス: uploads/acceptance/acceptance_{order_id}.pdf
    let pdf_dir = std::path::Path::new("uploads/acceptance");
    if let Err(e) = std::fs::create_dir_all(pdf_dir) {
        tracing::error!("PDFディレクトリ作成エラー: {:?}", e);
    }
    let pdf_path = pdf_dir.join(format!("acceptance_{}.pdf", order_id));
    if let Err(e) = std::fs::write(&pdf_path, &pdf_bytes) {
        tracing::error!("PDF保存エラー: {:?}", e);
        return axum::Json(serde_json::json!({ "status": "error", "error": "PDFの保存に失敗しました" }));
    }

    // DB更新: ステータス + 承諾日時 + ハッシュ + PDFパス
    let result = order_repo::finalize_order_acceptance(&pool, &order_id, &document_hash, pdf_path.to_str().unwrap_or("")).await;

    if let Err(e) = result {
        tracing::error!("承諾DB更新エラー: {:?}", e);
        return axum::Json(serde_json::json!({ "status": "error", "error": "データベース更新に失敗しました" }));
    }

    // 社内通知メール（token.rsの既存ロジックを再利用）
    send_order_approve_notification(&pool, &order_id, partner_id, &order.project_id).await;

    tracing::info!("[ポータル注文承諾] {} がパートナー {} により承諾", order_id, partner_id);
    axum::Json(serde_json::json!({ "status": "ok", "message": "注文書を承諾しました" }))
}

/// GET /api/portal/orders/{order_id}/pdf — 注文書PDF
pub async fn api_order_pdf(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(order_id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let order = order_repo::find_purchase_order_for_partner(&pool, &order_id, partner_id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Response::builder().status(StatusCode::NOT_FOUND)
            .body(Body::from("注文書が見つかりません")).expect("Response builder"),
    };

    let pdf_data = match crate::presentation::handlers::orders::build_purchase_order_pdf_data(&pool, &order).await {
        Ok(d) => d,
        Err(e) => return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("PDFデータ構築エラー: {}", e))).expect("Response builder"),
    };

    let gen = PdfGenerator::new();
    match gen.generate_purchase_order_pdf(&pdf_data) {
        Ok(pdf_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"order_{}.pdf\"", order_id))
            .body(Body::from(pdf_bytes)).expect("Response builder"),
        Err(e) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("PDF生成エラー: {}", e))).expect("Response builder"),
    }
}

/// GET /api/portal/orders/{order_id}/acceptance-pdf — 注文請書PDF
pub async fn api_acceptance_pdf(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(order_id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let order = order_repo::find_purchase_order_for_partner(&pool, &order_id, partner_id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Response::builder().status(StatusCode::NOT_FOUND)
            .body(Body::from("注文書が見つかりません")).expect("Response builder"),
    };

    let pdf_data = match crate::presentation::handlers::orders::build_purchase_order_pdf_data(&pool, &order).await {
        Ok(d) => d,
        Err(e) => return Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("PDFデータ構築エラー: {}", e))).expect("Response builder"),
    };

    let gen = PdfGenerator::new();
    match gen.generate_acceptance_pdf(&pdf_data) {
        Ok(pdf_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"acceptance_{}.pdf\"", order_id))
            .body(Body::from(pdf_bytes)).expect("Response builder"),
        Err(e) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("注文請書PDF生成エラー: {}", e))).expect("Response builder"),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 内部ヘルパー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 注文書承諾時の社内通知メール送信
async fn send_order_approve_notification(pool: &PgPool, order_id: &str, partner_id: &str, project_id: &str) {
    use crate::domain::services::email_service::{EmailService, compose_order_approve_email};

    let partner_name: String = order_repo::find_partner_name_email(pool, partner_id)
        .await.ok().flatten().map(|(name, _email)| name).unwrap_or_default();
    let project_name: String = order_repo::find_project_name(pool, project_id)
        .await.ok().flatten().unwrap_or_default();

    let email_svc = EmailService::new(pool.clone());
    let notify_email = crate::domain::services::email_service::get_notify_email(pool).await;
    let ctx = compose_order_approve_email(&partner_name, order_id, &project_name);

    if let Err(e) = email_svc.send_by_template("order_approve", &notify_email, None, &ctx).await {
        tracing::error!("注文書承諾通知メール送信エラー: {:?}", e);
    } else {
        tracing::info!("[ポータル注文承諾通知] {} が {} を承諾 → {} に通知", partner_name, order_id, notify_email);
    }
}
