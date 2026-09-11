use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use crate::domain::services::email_service::{EmailService, compose_invitation_email};
use crate::infrastructure::repositories::{order_repo, invite_repo};
use crate::presentation::middleware::role::AuthUser;
use super::{check_permission, InvitationRequest};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GET /api/portal/engineers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// パートナーに所属するエンジニア一覧
pub async fn list_engineers(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    // エンジニア本人の場合
    if let Some(engineer_id) = auth_user.engineer_id() {
        let engineer = order_repo::find_engineer_id_name(&pool, engineer_id).await;

        return match engineer {
            Ok(Some(e)) => Json(serde_json::json!({ "engineers": [{"id": e.0, "name": e.1}] })).into_response(),
            _ => (StatusCode::NOT_FOUND, "Engineer not found").into_response(),
        };
    }

    // パートナー担当者の場合
    let partner_id = match auth_user.partner_id() {
        Some(id) => id,
        None => return (StatusCode::FORBIDDEN, "Access denied").into_response(),
    };

    let engineers = order_repo::list_engineer_id_name_by_partner(&pool, partner_id).await;

    match engineers {
        Ok(list) => {
            let list: Vec<_> = list.into_iter().map(|(id, name)| serde_json::json!({"id": id, "name": name})).collect();
            Json(serde_json::json!({ "engineers": list })).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response(),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GET /api/engineers/options
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// スタッフ側「エンジニアを招待」モーダル用の全エンジニア一覧（配列を直接返す）。
///
/// 招待の実行自体（POST /api/invite）は check_permission() により実質Admin専用
/// （一般社員は engineer_id/partner_id を持たないため必ず403になる）だが、
/// このオプション一覧は /api/masters/{table} をAdmin専用化した際の代替として新設した
/// 軽量エンドポイント。一般社員がモーダルを開いても一覧取得自体は失敗しないよう
/// employee_routes 配下に置く（実際の招待送信は引き続きAdminのみ成功する）。
pub async fn api_engineer_options(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let engineers = order_repo::list_active_engineer_options(&pool).await;

    match engineers {
        Ok(list) => {
            let list: Vec<_> = list.into_iter().map(|(id, name, email)| serde_json::json!({"id": id, "name": name, "email": email})).collect();
            Json(list).into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response(),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// POST /api/portal/invite
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// エンジニアへの招待メール送信
pub async fn invite_engineer(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<InvitationRequest>,
) -> impl IntoResponse {
    // 権限チェック
    if !check_permission(&pool, &auth_user, body.engineer_id).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "このエンジニアへのアクセス権がありません"}))).into_response();
    }

    // メールアドレスのバリデーション（簡易）
    if !body.email.contains('@') {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "無効なメールアドレスです"}))).into_response();
    }

    // エンジニア情報の更新（メールアドレス保存）
    let res = order_repo::update_engineer_email(&pool, body.engineer_id, &body.email).await;

    if res.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "エンジニア情報の更新に失敗しました"}))).into_response();
    }

    // エンジニア情報取得（名前・パートナーID）
    let engineer_info = order_repo::find_engineer_name_partner_id(&pool, body.engineer_id)
        .await
        .ok()
        .flatten();

    let (engineer_name, partner_id) = match engineer_info {
        Some((name, Some(pid))) => (name, pid),
        Some((name, None)) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("{}はパートナーに紐づいていません", name)}))).into_response();
        }
        None => {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "エンジニアが見つかりません"}))).into_response();
        }
    };

    // 招待トークンの生成（s_partner_invitation テーブルに統一）
    let token = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::days(7);

    let res = invite_repo::insert_invitation(&pool, &partner_id, token, &body.email, &engineer_name, expires_at).await;

    if let Err(e) = res {
        tracing::error!("招待トークン生成エラー: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "招待トークンの生成に失敗しました"}))).into_response();
    }

    // 招待メール送信
    let base_url = std::env::var("BASE_URL")
        .or_else(|_| std::env::var("FRONTEND_URL"))
        .unwrap_or_else(|_| "http://localhost:8111".to_string());
    let invite_url = format!("{}/invite/{}", base_url, token);

    let email_service = EmailService::new(pool.clone());
    let context = compose_invitation_email(&engineer_name, &invite_url);

    match email_service.send_by_template("INVITE_ENGINEER", &body.email, None, &context).await {
        Ok(_) => Json(serde_json::json!({ "success": true, "message": "招待メールを送信しました" })).into_response(),
        Err(e) => {
            tracing::error!("招待メール送信失敗: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "メール送信に失敗しました" }))).into_response()
        }
    }
}
