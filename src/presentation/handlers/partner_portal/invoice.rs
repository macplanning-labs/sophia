use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
};
use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::middleware::role::AuthUser;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 請求書（支払通知書）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// GET /api/portal/notices — 請求書一覧
///
/// カラム: notice_id, project_name, target_month, notice_date, total(税込), status
pub async fn api_notices(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown");

    let rows = order_repo::list_portal_notices(&pool, partner_id).await.unwrap_or_default();

    let notices: Vec<serde_json::Value> = rows.into_iter().map(|(notice_id, project_name, target_month, notice_date, total, partner_accepted_at, uuid)| {
        serde_json::json!({
            "notice_id": notice_id,
            "project_name": project_name,
            "target_month": target_month.format("%Y年%m月").to_string(),
            "notice_date": notice_date.format("%Y/%m/%d").to_string(),
            "total": total,
            "status": if partner_accepted_at.is_some() { "承諾済" } else { "送付済" },
            "confirmed_at": partner_accepted_at,
            "uuid": uuid.to_string(),
        })
    }).collect();

    axum::Json(serde_json::json!({ "notices": notices }))
}

/// POST /api/portal/notices/{notice_id}/confirm — 請求書承諾
///
/// 処理: confirmed_at更新 → 請求書PDF永続保存 → SHA256ハッシュ → 注文書ステータス更新 → 社内通知メール
pub async fn api_notice_confirm(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(notice_id): Path<String>,
) -> impl IntoResponse {
    use sha2::{Sha256, Digest};

    let partner_id = auth_user.partner_id().unwrap_or("unknown");

    // 権限チェック
    let notice = order_repo::find_payment_notice_for_partner(&pool, &notice_id, partner_id).await;

    let notice = match notice {
        Ok(Some(n)) => n,
        _ => return axum::Json(serde_json::json!({ "status": "error", "error": "請求書が見つかりません" })),
    };

    if notice.partner_accepted_at.is_some() {
        return axum::Json(serde_json::json!({ "status": "error", "error": "この請求書は既に承諾済みです" }));
    }

    // 請求書（パートナー名義）PDFを生成 → ハッシュ計算
    // ※ 当社がパートナーに代わり作成した請求書。承諾によりパートナー発行扱い。
    let pdf_bytes = match crate::presentation::handlers::notices::generate_partner_invoice_pdf_bytes(&pool, &notice).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("請求書PDF生成エラー: {}", e);
            return axum::Json(serde_json::json!({ "status": "error", "error": "PDF生成に失敗しました" }));
        }
    };

    // SHA256ハッシュ（改ざん防止 — 電子帳簿保存法）
    let mut hasher = Sha256::new();
    hasher.update(&pdf_bytes);
    let document_hash = format!("{:x}", hasher.finalize());

    // PDF永続保存: uploads/invoices/invoice_{notice_id}.pdf
    let pdf_dir = std::path::Path::new("uploads/invoices");
    if let Err(e) = std::fs::create_dir_all(pdf_dir) {
        tracing::error!("PDFディレクトリ作成エラー: {:?}", e);
    }
    let pdf_path = pdf_dir.join(format!("invoice_{}.pdf", notice_id));
    if let Err(e) = std::fs::write(&pdf_path, &pdf_bytes) {
        tracing::error!("PDF保存エラー: {:?}", e);
        return axum::Json(serde_json::json!({ "status": "error", "error": "PDFの保存に失敗しました" }));
    }

    // DB更新: partner_accepted_at + document_hash
    if let Err(e) = order_repo::confirm_payment_notice(&pool, &notice_id, &document_hash).await {
        tracing::error!("承諾DB更新エラー: {:?}", e);
        return axum::Json(serde_json::json!({ "status": "error", "error": "データベース更新に失敗しました" }));
    }

    // 関連注文書ステータスを NOTICE_CONFIRMED に更新
    if let Err(e) = order_repo::update_purchase_order_status(&pool, &notice.purchase_order_id, "NOTICE_CONFIRMED").await {
        tracing::error!("注文書ステータス更新エラー: {:?}", e);
    }

    // 社内通知メール
    send_invoice_approve_notification(&pool, &notice_id, partner_id, notice.target_month, notice.total as i64).await;

    tracing::info!("[ポータル請求承諾] {} がパートナー {} により承諾 (hash: {})", notice_id, partner_id, &document_hash[..8]);
    axum::Json(serde_json::json!({ "status": "ok", "message": "請求書を承諾しました" }))
}

/// GET /api/portal/notices/{notice_id}/invoice-pdf — 請求書PDF（パートナー名義）
pub async fn api_invoice_pdf(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(notice_id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};

    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let notice = order_repo::find_payment_notice_for_partner(&pool, &notice_id, partner_id).await.ok().flatten();

    let notice = match notice {
        Some(n) => n,
        None => return Response::builder().status(StatusCode::NOT_FOUND)
            .body(Body::from("請求書が見つかりません")).expect("Response builder"),
    };

    match crate::presentation::handlers::notices::generate_partner_invoice_pdf_bytes(&pool, &notice).await {
        Ok(pdf_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"invoice_{}.pdf\"", notice_id))
            .body(Body::from(pdf_bytes)).expect("Response builder"),
        Err(e) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(e)).expect("Response builder"),
    }
}

/// GET /api/portal/notices/{notice_id}/payment-notice-pdf — 支払通知書PDF
pub async fn api_payment_notice_pdf(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(notice_id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};

    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let notice = order_repo::find_payment_notice_for_partner(&pool, &notice_id, partner_id).await.ok().flatten();

    let notice = match notice {
        Some(n) => n,
        None => return Response::builder().status(StatusCode::NOT_FOUND)
            .body(Body::from("支払通知書が見つかりません")).expect("Response builder"),
    };

    match crate::presentation::handlers::notices::generate_payment_notice_pdf_bytes(&pool, &notice).await {
        Ok(pdf_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"notice_{}.pdf\"", notice_id))
            .body(Body::from(pdf_bytes)).expect("Response builder"),
        Err(e) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(e)).expect("Response builder"),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 内部ヘルパー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 請求書承諾時の社内通知メール送信
async fn send_invoice_approve_notification(pool: &PgPool, notice_id: &str, partner_id: &str, target_month: chrono::NaiveDate, total: i64) {
    use crate::domain::services::email_service::{EmailService, compose_invoice_approve_email};

    let partner_name: String = order_repo::find_partner_name_email(pool, partner_id)
        .await.ok().flatten().map(|(name, _email)| name).unwrap_or_default();

    let email_svc = EmailService::new(pool.clone());
    let notify_email = crate::domain::services::email_service::get_notify_email(pool).await;
    let ctx = compose_invoice_approve_email(&partner_name, notice_id, &target_month.format("%Y年%m月").to_string(), total);

    if let Err(e) = email_svc.send_by_template("invoice_approve", &notify_email, None, &ctx).await {
        tracing::error!("請求書承諾通知メール送信エラー: {:?}", e);
    } else {
        tracing::info!("[ポータル請求承諾通知] {} が {} を承諾 → {} に通知", partner_name, notice_id, notify_email);
    }
}
