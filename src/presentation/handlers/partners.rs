/// presentation/handlers/partners.rs — パートナー管理ハンドラ（Askama化）

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::infrastructure::repositories::partner_repo::{self, PartnerFormInput};

/// パートナー詳細（GET /partners/:id）
pub async fn detail(
    State(pool): State<PgPool>,
    Path(partner_id): Path<String>,
) -> impl IntoResponse {
    let partner = partner_repo::find_by_id(&pool, &partner_id)
        .await
        .ok()
        .flatten();

    match partner {
        Some(p) => axum::response::Html(format!(
            r#"<h4>{}</h4><p>ID: {}, Email: {}, Tel: {}</p><a href="/partners">一覧</a>"#,
            p.name, p.partner_id, p.email, p.tel
        )).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

/// 新規作成フォーム（GET /partners/new）
pub async fn new_form() -> impl IntoResponse {
    axum::response::Html(partner_form_html("新規パートナー", "", "", "", "", ""))
}

/// 新規作成処理（POST /partners）
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<PartnerForm>,
) -> impl IntoResponse {
    let next_id = partner_repo::next_partner_id(&pool).await.unwrap_or_else(|_| "0000000001".to_string());

    if let Err(e) = partner_repo::create(&pool, &next_id, PartnerFormInput {
        name: &form.name,
        email: &form.email,
        tel: &form.tel,
        address: &form.address,
        representative_name: &form.representative_name,
    }).await { tracing::error!("DB error: {:?}", e); }

    Redirect::to("/partners")
}

/// 編集フォーム（GET /partners/:id/edit）
pub async fn edit_form(
    State(pool): State<PgPool>,
    Path(partner_id): Path<String>,
) -> impl IntoResponse {
    let partner = partner_repo::find_by_id(&pool, &partner_id)
        .await
        .ok()
        .flatten();

    match partner {
        Some(p) => axum::response::Html(partner_form_html(
            &format!("{} の編集", p.name),
            &p.name, &p.email, &p.tel, &p.address, &p.representative_name,
        )).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

/// 更新処理（POST /partners/:id）
pub async fn update(
    State(pool): State<PgPool>,
    Path(partner_id): Path<String>,
    Form(form): Form<PartnerForm>,
) -> impl IntoResponse {
    if let Err(e) = partner_repo::update(&pool, &partner_id, PartnerFormInput {
        name: &form.name,
        email: &form.email,
        tel: &form.tel,
        address: &form.address,
        representative_name: &form.representative_name,
    }).await { tracing::error!("DB error: {:?}", e); }

    Redirect::to("/partners")
}

#[derive(serde::Deserialize)]
pub struct PartnerForm {
    name: String,
    email: String,
    tel: String,
    address: String,
    representative_name: String,
}

fn partner_form_html(title: &str, name: &str, email: &str, tel: &str, addr: &str, rep: &str) -> String {
    format!(
        r##"<!DOCTYPE html><html lang="ja"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>{title} | Sophia</title>
<link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css" rel="stylesheet">
</head><body><div class="container-fluid p-4">
<h4>{title}</h4>
<form method="POST">
<div class="card"><div class="card-body">
    <div class="mb-3"><label class="form-label">名前</label><input type="text" name="name" class="form-control" value="{name}" required></div>
    <div class="mb-3"><label class="form-label">メール</label><input type="email" name="email" class="form-control" value="{email}" required></div>
    <div class="mb-3"><label class="form-label">電話</label><input type="text" name="tel" class="form-control" value="{tel}"></div>
    <div class="mb-3"><label class="form-label">住所</label><input type="text" name="address" class="form-control" value="{addr}"></div>
    <div class="mb-3"><label class="form-label">代表者</label><input type="text" name="representative_name" class="form-control" value="{rep}"></div>
</div></div>
<div class="mt-3">
    <button type="submit" class="btn btn-primary me-2">保存</button>
    <a href="/partners" class="btn btn-outline-secondary">キャンセル</a>
</div>
</form></div></body></html>"##,
        title = title, name = name, email = email, tel = tel, addr = addr, rep = rep,
    )
}
