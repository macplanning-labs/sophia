/// expenses/legacy.rs — 旧SSRハンドラ
///
/// `create`は`routes.rs`に一切ルーティングされていない到達不能コード（相当機能は`api::api_create`）。
/// `submit`/`approve`/`reject`はルーティングされているが、DRAFT状態のヘッダーを作る経路が
/// 存在しないため実質的に到達しない。互換のため残す。

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::domain::models::expense::ExpenseRequestForm;
use crate::infrastructure::repositories::expense_repo;

/// POST /expenses — 作成（未ルーティングのdead code。相当機能は`api::api_create`）
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<ExpenseRequestForm>,
) -> impl IntoResponse {
    let result = expense_repo::insert_draft(&pool, &form).await;

    match result {
        Ok(id) => Redirect::to(&format!("/expenses/{}", id)),
        Err(_) => Redirect::to("/expenses/new"),
    }
}

/// POST /expenses/{id}/submit — 申請
pub async fn submit(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = expense_repo::submit(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/expenses/{}", id))
}

/// POST /expenses/{id}/approve — 承認
pub async fn approve(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = expense_repo::approve(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/expenses/{}", id))
}

/// POST /expenses/{id}/reject — 差戻し
pub async fn reject(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = expense_repo::reject(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/expenses/{}", id))
}
