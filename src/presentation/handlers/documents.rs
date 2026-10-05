/// presentation/handlers/documents.rs — 帳票横断検索API
///
/// 受注書・発注書・請求書・支払通知を横断検索する管理者専用API。
/// `GET /api/v1/documents/search?q=…&types=order,received_order,invoice,notice&limit=20`

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::IntoParams;

use crate::infrastructure::repositories::documents_search_repo;
use crate::presentation::api_response::ApiError;

/// 検索リクエスト
#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchDocumentsQuery {
    /// 検索クエリ（1文字以上、空白のみはNG）
    pub q: Option<String>,
    /// 帳票種別（カンマ区切り: "received_order,order,invoice,notice"）
    pub types: Option<String>,
    /// 最大件数（既定20、最大50）
    pub limit: Option<i32>,
}

/// 検索応答（リポジトリの型をそのまま使用、スキーマとして公開）
pub type SearchDocumentsResponse = documents_search_repo::DocumentSearchResult;

/// 帳票横断検索
/// `GET /api/v1/documents/search?q=…&types=…&limit=…`
#[utoipa::path(
    get,
    path = "/api/v1/documents/search",
    params(SearchDocumentsQuery),
    responses(
        (status = 200, description = "検索成功", body = Vec<SearchDocumentsResponse>),
        (status = 400, description = "不正なパラメータ", body = ApiError),
        (status = 500, description = "サーバーエラー"),
    ),
)]
pub async fn api_search_documents(
    State(pool): State<PgPool>,
    Query(params): Query<SearchDocumentsQuery>,
) -> Result<Json<Vec<SearchDocumentsResponse>>, (StatusCode, Json<ApiError>)> {
    // q パラメータの検証（必須、空白のみはNG）
    let query = params.q.as_deref().unwrap_or("").trim();
    if query.is_empty() {
        return Err(ApiError::bad_request(
            "Search query (q) cannot be empty or whitespace",
        ));
    }

    // limit パラメータの検証と処理
    let mut limit = params.limit.unwrap_or(20);
    if limit < 1 {
        limit = 20;
    } else if limit > 50 {
        limit = 50;
    }

    // types パラメータの処理（カンマ区切り）
    let types_str = params.types.as_deref().unwrap_or("");
    let types: Vec<&str> = if types_str.is_empty() {
        // デフォルト: 4種類すべて
        vec!["received_order", "order", "invoice", "notice"]
    } else {
        types_str.split(',').map(|s| s.trim()).collect()
    };

    // types の検証（不明な type は 400）
    let valid_types = ["received_order", "order", "invoice", "notice"];
    for t in &types {
        if !valid_types.contains(t) {
            return Err(ApiError::bad_request(format!("Unknown document type: {}", t)));
        }
    }

    // リポジトリで検索実行
    match documents_search_repo::search_documents(&pool, query, &types, limit).await {
        Ok(results) => Ok(Json(results)),
        Err(e) => {
            tracing::error!("search_documents failed: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    success: false,
                    error: "Search failed".to_string(),
                }),
            ))
        }
    }
}
