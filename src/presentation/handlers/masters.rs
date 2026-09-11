/// presentation/handlers/masters.rs — マスタメンテナンス（メタデータ駆動）
///
/// テーブル定義・DBアクセスは `infrastructure::repositories::master_repo` に集約。
/// ここではHTTPリクエスト/レスポンスの変換のみを行う。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use sqlx::PgPool;

use crate::infrastructure::repositories::master_repo::{self, ColDef};

/// メタデータをJSON化してフロントに渡す
#[derive(Serialize)]
struct TableMeta {
    key: &'static str,
    label: &'static str,
    category: &'static str,
    pk_column: &'static str,
    list_columns: Vec<ColMeta>,
    form_columns: Vec<FormColMeta>,
}

#[derive(Serialize)]
struct ColMeta {
    name: &'static str,
    label: &'static str,
}

#[derive(Serialize, Clone)]
struct FormColMeta {
    name: &'static str,
    label: &'static str,
    input_type: &'static str,
    required: bool,
    is_pk: bool,
    options: Vec<OptionMeta>,
    #[serde(skip_serializing_if = "str::is_empty")]
    fk_table: &'static str,
    #[serde(skip_serializing_if = "str::is_empty")]
    fk_value: &'static str,
    #[serde(skip_serializing_if = "str::is_empty")]
    fk_label: &'static str,
}

#[derive(Serialize, Clone)]
struct OptionMeta {
    value: String,
    label: String,
}

fn build_table_meta() -> Vec<TableMeta> {
    master_repo::MASTER_TABLES.iter().map(|t| {
        let pk_col = t.columns.iter().find(|c| c.is_pk).map(|c| c.name).unwrap_or("id");
        TableMeta {
            key: t.key,
            label: t.label,
            category: t.category,
            pk_column: pk_col,
            list_columns: t.columns.iter()
                .filter(|c| c.in_list)
                .map(|c| ColMeta { name: c.name, label: c.label })
                .collect(),
            form_columns: t.columns.iter()
                .filter(|c| c.in_form)
                .map(|c| FormColMeta {
                    name: c.name,
                    label: c.label,
                    input_type: c.input_type,
                    required: c.required,
                    is_pk: c.is_pk,
                    options: c.options.iter().map(|(v, l)| OptionMeta { value: v.to_string(), label: l.to_string() }).collect(),
                    fk_table: c.fk_table,
                    fk_value: c.fk_value,
                    fk_label: c.fk_label,
                })
                .collect(),
        }
    }).collect()
}

// ── API: メタデータ取得 ──

pub async fn api_meta() -> impl IntoResponse {
    Json(build_table_meta())
}

// ── API: FK選択肢取得 ──

/// GET /api/masters/{table}/fk-options
///
/// 指定マスタのフォームが持つ全fk_selectカラムについて、参照先テーブルから
/// (value, label) の選択肢一覧をまとめて返す。フォーム側は `{ "client_id": [...], ... }`
/// という形で受け取り、対応するカラム名のドロップダウンを埋める。
pub async fn api_fk_options(
    State(pool): State<PgPool>,
    Path(table_key): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;

    let mut result = serde_json::Map::new();
    for (col_name, rows) in master_repo::fetch_fk_options(&pool, def).await {
        let options: Vec<OptionMeta> = rows.into_iter()
            .map(|(value, label)| OptionMeta { value, label })
            .collect();
        result.insert(col_name.to_string(), serde_json::to_value(options).unwrap_or_default());
    }

    Ok(Json(serde_json::Value::Object(result)))
}

// ── API: 一覧取得 ──

pub async fn api_list(
    State(pool): State<PgPool>,
    Path(table_key): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;

    let rows = master_repo::fetch_list(&pool, def)
        .await
        .map_err(|e| {
            tracing::error!("masters list error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // sqlx::Row → serde_json::Value に手動変換
    let result: Vec<serde_json::Value> = rows.iter().map(|row| {
        let mut obj = serde_json::Map::new();
        for c in def.columns.iter().filter(|c| c.in_list || c.is_pk) {
            let val = master_repo::try_get_value(row, c.name);
            obj.insert(c.name.to_string(), val);
        }
        // list_extra_select のカラム（例: client_name）
        if !def.list_extra_select.is_empty() {
            // ", c.name as client_name" → "client_name"
            for part in def.list_extra_select.split(',') {
                let part = part.trim();
                if let Some(alias) = part.split(" as ").nth(1) {
                    let alias = alias.trim();
                    let val = master_repo::try_get_value(row, alias);
                    obj.insert(alias.to_string(), val);
                }
            }
        }
        serde_json::Value::Object(obj)
    }).collect();

    Ok(Json(result))
}

// ── API: 詳細取得 ──

pub async fn api_get(
    State(pool): State<PgPool>,
    Path((table_key, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;
    let pk_col = def.columns.iter().find(|c| c.is_pk).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let all_cols: Vec<&str> = def.columns.iter()
        .filter(|c| c.in_form || c.is_pk)
        .map(|c| c.name)
        .collect();

    let row = master_repo::fetch_detail(&pool, def, &id, pk_col, &all_cols)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut obj = serde_json::Map::new();
    for col_name in &all_cols {
        obj.insert(col_name.to_string(), master_repo::try_get_value(&row, col_name));
    }

    Ok(Json(serde_json::Value::Object(obj)))
}

// ── API: 更新 ──

/// フォーム項目のうち登録番号（T+13桁）系カラムの形式を検証する。
/// 不正な場合はユーザー向けエラーメッセージを返す。
fn validate_registration_no(body: &serde_json::Value) -> Result<(), String> {
    if let Some(v) = body.get("registration_no").and_then(|v| v.as_str()) {
        if !crate::domain::value_objects::is_valid_qualified_invoice_registration_no(v) {
            return Err("適格請求書発行事業者登録番号は「T」+数字13桁の形式で入力してください（例: T1234567890123）".to_string());
        }
    }
    Ok(())
}

pub async fn api_update(
    State(pool): State<PgPool>,
    Path((table_key, id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;
    let pk_col: &ColDef = def.columns.iter().find(|c| c.is_pk).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Err(msg) = validate_registration_no(&body) {
        return Ok(Json(serde_json::json!({"ok": false, "error": msg})));
    }

    master_repo::execute_update(&pool, def, &id, pk_col, &body).await.map_err(|e| {
        tracing::error!("masters update error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ── API: 新規作成 ──

pub async fn api_create(
    State(pool): State<PgPool>,
    Path(table_key): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;

    if let Err(msg) = validate_registration_no(&body) {
        return Ok(Json(serde_json::json!({"ok": false, "error": msg})));
    }

    master_repo::execute_create(&pool, def, body).await.map_err(|e| {
        tracing::error!("masters create error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({"ok": true})))
}

// ── API: 削除 ──

pub async fn api_delete(
    State(pool): State<PgPool>,
    Path((table_key, id)): Path<(String, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let def = master_repo::find_table(&table_key).ok_or(StatusCode::NOT_FOUND)?;
    let pk_col = def.columns.iter().find(|c| c.is_pk).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    match master_repo::execute_delete(&pool, def, &id, pk_col).await {
        Ok(()) => Ok(Json(serde_json::json!({"ok": true}))),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("foreign key") || msg.contains("violates") {
                Ok(Json(serde_json::json!({"ok": false, "error": "他のデータから参照されているため削除できません"})))
            } else {
                tracing::error!("masters delete error: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
