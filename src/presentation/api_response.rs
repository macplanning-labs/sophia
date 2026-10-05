use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

/// `anyhow::Error` を JSON エラーレスポンス（500 + `ApiError`）に変換する共通ラッパー。
///
/// ハンドラで `Result<impl IntoResponse, AppError>` を戻り値にし `?` でDBエラー等を
/// 伝播させると、握りつぶし（`let _ =` / `.unwrap_or_default()`）を避けつつ
/// ログ出力とユーザー向けエラーレスポンスを一箇所（`into_response`）に集約できる。
#[derive(Debug)]
pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        // 詳細（SQLエラー文・外部APIレスポンス等）はサーバーログにのみ残し、
        // クライアントへはDB構造や連携先の内部情報を含みうる生メッセージを返さない
        tracing::error!(
            error_code = "ERR_APP_INTERNAL",
            error = %self.0,
            "handler failed"
        );
        ApiError::internal("サーバーエラーが発生しました。しばらく経ってから再度お試しください。").into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

/// SPA用 JSON API の共通レスポンス型
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> axum::Json<Self> {
        axum::Json(Self {
            success: true,
            message: None,
            data: Some(data),
        })
    }

    pub fn ok_msg(message: impl Into<String>) -> axum::Json<Self> {
        axum::Json(Self {
            success: true,
            message: Some(message.into()),
            data: None,
        })
    }
}

/// エラーレスポンス
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub success: bool,
    pub error: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::BAD_REQUEST,
            axum::Json(Self { success: false, error: msg.into() }),
        )
    }

    pub fn not_found(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::NOT_FOUND,
            axum::Json(Self { success: false, error: msg.into() }),
        )
    }

    pub fn conflict(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::CONFLICT,
            axum::Json(Self { success: false, error: msg.into() }),
        )
    }

    pub fn internal(msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(Self { success: false, error: msg.into() }),
        )
    }
}

/// Created レスポンス（IDを返す）
#[derive(Debug, Serialize)]
pub struct CreatedResponse {
    pub success: bool,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CreatedResponse {
    pub fn new(id: impl Into<serde_json::Value>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::CREATED,
            axum::Json(Self {
                success: true,
                id: id.into(),
                message: None,
            }),
        )
    }

    pub fn with_msg(id: impl Into<serde_json::Value>, msg: impl Into<String>) -> (StatusCode, axum::Json<Self>) {
        (
            StatusCode::CREATED,
            axum::Json(Self {
                success: true,
                id: id.into(),
                message: Some(msg.into()),
            }),
        )
    }
}

/// 成功レスポンス（ステータス変更・削除等）
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

impl SuccessResponse {
    pub fn new(msg: impl Into<String>) -> axum::Json<Self> {
        axum::Json(Self {
            success: true,
            message: msg.into(),
        })
    }
}

impl IntoResponse for SuccessResponse {
    fn into_response(self) -> axum::response::Response {
        axum::Json(self).into_response()
    }
}
