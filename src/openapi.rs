/// OpenAPI スキーマ定義（utoipa）— 取引先 /v1 の4エンドポイント

use serde_json::json;

pub fn build_v1_api() -> serde_json::Value {
    json!({
        "openapi": "3.0.0",
        "info": {
            "title": "Sophia /v1 API",
            "version": "1.0.0"
        },
        "servers": [
            {
                "url": "/v1",
                "description": "取引先向け REST API"
            }
        ],
        "paths": {
            "/invoices": {
                "get": {
                    "tags": ["invoices"],
                    "summary": "請求書一覧",
                    "operationId": "list_invoices",
                    "parameters": [
                        {
                            "name": "from",
                            "in": "query",
                            "description": "開始日（YYYY-MM-DD形式）",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "to",
                            "in": "query",
                            "description": "終了日（YYYY-MM-DD形式）",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "min_amount",
                            "in": "query",
                            "description": "最小金額",
                            "schema": {
                                "type": "integer",
                                "format": "int32"
                            }
                        },
                        {
                            "name": "max_amount",
                            "in": "query",
                            "description": "最大金額",
                            "schema": {
                                "type": "integer",
                                "format": "int32"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "請求書一覧",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "$ref": "#/components/schemas/ApiInvoiceResponse"
                                        }
                                    }
                                }
                            }
                        },
                        "403": {
                            "description": "認可不可",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "サーバーエラー",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        }
                    },
                    "security": [{"ApiKeyAuth": []}]
                }
            },
            "/invoices/{id}": {
                "get": {
                    "tags": ["invoices"],
                    "summary": "請求書詳細",
                    "operationId": "get_invoice",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "請求書ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "請求書詳細",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiInvoiceResponse"
                                    }
                                }
                            }
                        },
                        "403": {
                            "description": "認可不可",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "見つかりません",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "サーバーエラー",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        }
                    },
                    "security": [{"ApiKeyAuth": []}]
                }
            },
            "/invoices/{id}/accept": {
                "post": {
                    "tags": ["invoices"],
                    "summary": "請求書承諾",
                    "operationId": "accept_invoice",
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "description": "請求書ID",
                            "required": true,
                            "schema": {
                                "type": "integer",
                                "format": "int64"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "承諾完了"
                        },
                        "403": {
                            "description": "認可不可",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "見つかりません",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "409": {
                            "description": "既に承諾済み",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "サーバーエラー",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        }
                    },
                    "security": [{"ApiKeyAuth": []}]
                }
            },
            "/orders": {
                "post": {
                    "tags": ["orders"],
                    "summary": "発注データ送信",
                    "operationId": "submit_order",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ApiOrderSubmitRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "発注受付完了",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiOrderSubmitResponse"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "不正なパラメータ",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "403": {
                            "description": "認可不可",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "サーバーエラー",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ApiErrorResponse"
                                    }
                                }
                            }
                        }
                    },
                    "security": [{"ApiKeyAuth": []}]
                }
            }
        },
        "components": {
            "schemas": {
                "ApiInvoiceResponse": {
                    "type": "object",
                    "required": ["invoice_id", "issue_date", "seller", "buyer", "lines", "tax_summary", "payable_amount", "status"],
                    "properties": {
                        "invoice_id": {
                            "type": "string"
                        },
                        "issue_date": {
                            "type": "string",
                            "format": "date"
                        },
                        "due_date": {
                            "type": "string",
                            "format": "date",
                            "nullable": true
                        },
                        "seller": {
                            "$ref": "#/components/schemas/ApiParty"
                        },
                        "buyer": {
                            "$ref": "#/components/schemas/ApiParty"
                        },
                        "lines": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ApiInvoiceLine"
                            }
                        },
                        "tax_summary": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ApiTaxSubtotal"
                            }
                        },
                        "payable_amount": {
                            "type": "integer",
                            "format": "int32"
                        },
                        "status": {
                            "type": "string"
                        },
                        "client_accepted_at": {
                            "type": "string",
                            "format": "date-time",
                            "nullable": true
                        }
                    }
                },
                "ApiParty": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {
                            "type": "string"
                        },
                        "postal_code": {
                            "type": "string",
                            "nullable": true
                        },
                        "address": {
                            "type": "string",
                            "nullable": true
                        },
                        "contact": {
                            "type": "string",
                            "nullable": true
                        }
                    }
                },
                "ApiInvoiceLine": {
                    "type": "object",
                    "required": ["description", "quantity", "unit_price", "amount"],
                    "properties": {
                        "description": {
                            "type": "string"
                        },
                        "quantity": {
                            "type": "string"
                        },
                        "unit_price": {
                            "type": "integer",
                            "format": "int32"
                        },
                        "amount": {
                            "type": "integer",
                            "format": "int32"
                        }
                    }
                },
                "ApiTaxSubtotal": {
                    "type": "object",
                    "required": ["tax_rate", "taxable_amount", "tax_amount"],
                    "properties": {
                        "tax_rate": {
                            "type": "string"
                        },
                        "taxable_amount": {
                            "type": "integer",
                            "format": "int32"
                        },
                        "tax_amount": {
                            "type": "integer",
                            "format": "int32"
                        }
                    }
                },
                "ApiOrderSubmitRequest": {
                    "type": "object",
                    "required": ["order_date", "work_start", "work_end", "items"],
                    "properties": {
                        "order_date": {
                            "type": "string",
                            "format": "date"
                        },
                        "work_start": {
                            "type": "string",
                            "format": "date"
                        },
                        "work_end": {
                            "type": "string",
                            "format": "date"
                        },
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ApiOrderItem"
                            }
                        },
                        "remarks": {
                            "type": "string",
                            "nullable": true
                        }
                    }
                },
                "ApiOrderItem": {
                    "type": "object",
                    "required": ["engineer_id", "description", "unit_price", "quantity"],
                    "properties": {
                        "engineer_id": {
                            "type": "string"
                        },
                        "description": {
                            "type": "string"
                        },
                        "unit_price": {
                            "type": "integer",
                            "format": "int32"
                        },
                        "quantity": {
                            "type": "integer",
                            "format": "int32"
                        }
                    }
                },
                "ApiOrderSubmitResponse": {
                    "type": "object",
                    "required": ["received_order_id", "received_order_no"],
                    "properties": {
                        "received_order_id": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "received_order_no": {
                            "type": "string"
                        }
                    }
                },
                "ApiErrorResponse": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": {
                            "type": "string"
                        },
                        "message": {
                            "type": "string",
                            "nullable": true
                        }
                    }
                }
            },
            "securitySchemes": {
                "ApiKeyAuth": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "API Key",
                    "description": "Authorization: Bearer sk_live_... または sk_test_..."
                }
            }
        },
        "tags": [
            {
                "name": "invoices",
                "description": "請求書関連 API"
            },
            {
                "name": "orders",
                "description": "発注関連 API"
            }
        ]
    })
}

/// 社内向け OpenAPI スキーマ — 段階展開
///
/// 認証は Cookie `sophia_jwt`（Admin）。実キーや接続文字列はスキーマに載せない。
pub fn build_internal_v1_api() -> serde_json::Value {
    let mut api = json!({
        "openapi": "3.0.0",
        "info": {
            "title": "Sophia /api/v1 Internal API",
            "version": "1.0.0",
            "description": "社内向け REST API（開発・運用用）。取引先向けの /v1 API とは別。第1波 api-keys、第2波 notices/invoices/timesheets。"
        },
        "servers": [
            {
                "url": "/api/v1",
                "description": "社内向け REST API"
            }
        ],
        "paths": {
            "/notices": {
                "get": {
                    "tags": ["notices"],
                    "summary": "支払通知書一覧",
                    "operationId": "list_notices",
                    "security": [{"CookieAuth": []}],
                    "parameters": [
                        {
                            "name": "partner",
                            "in": "query",
                            "description": "パートナーID（オプション）",
                            "schema": {"type": "string"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "支払通知書一覧",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "$ref": "#/components/schemas/NoticeResponse"
                                        }
                                    }
                                }
                            }
                        },
                        "401": {"description": "認証失敗"},
                        "500": {"description": "サーバーエラー"}
                    }
                }
            },
            "/invoices": {
                "get": {
                    "tags": ["invoices"],
                    "summary": "請求書一覧",
                    "operationId": "list_invoices_internal",
                    "security": [{"CookieAuth": []}],
                    "responses": {
                        "200": {
                            "description": "請求書一覧",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "$ref": "#/components/schemas/InvoiceResponse"
                                        }
                                    }
                                }
                            }
                        },
                        "401": {"description": "認証失敗"},
                        "500": {"description": "サーバーエラー"}
                    }
                }
            },
            "/timesheets": {
                "get": {
                    "tags": ["timesheets"],
                    "summary": "稼働報告一覧",
                    "operationId": "list_timesheets",
                    "security": [{"CookieAuth": []}],
                    "responses": {
                        "200": {
                            "description": "稼働報告一覧",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/TimesheetListResponse"
                                    }
                                }
                            }
                        },
                        "401": {"description": "認証失敗"},
                        "500": {"description": "サーバーエラー"}
                    }
                }
            },
            "/settings/api-keys": {
                "get": {
                    "tags": ["settings"],
                    "summary": "API キー一覧",
                    "operationId": "list_api_keys",
                    "security": [{"CookieAuth": []}],
                    "parameters": [
                        {
                            "name": "client_id",
                            "in": "query",
                            "required": true,
                            "description": "クライアント（取引先）ID",
                            "schema": {"type": "integer", "format": "int64"}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "API キー一覧",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": {"type": "string", "format": "uuid"},
                                                "key_prefix": {"type": "string", "description": "sk_live_ または sk_test_ で始まる接頭辞"},
                                                "scope": {"type": "string", "enum": ["READ", "READ_WRITE"]},
                                                "name": {"type": "string"},
                                                "is_active": {"type": "boolean"},
                                                "created_at": {"type": "string", "format": "date-time"},
                                                "revoked_at": {"type": "string", "format": "date-time", "nullable": true}
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "400": {"description": "client_id 未指定など"},
                        "401": {"description": "認証失敗"},
                        "403": {"description": "権限不足（Admin 以外）"}
                    }
                },
                "post": {
                    "tags": ["settings"],
                    "summary": "API キー発行",
                    "operationId": "generate_api_key",
                    "security": [{"CookieAuth": []}],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["client_id", "name", "scope", "environment"],
                                    "properties": {
                                        "client_id": {"type": "integer", "format": "int64"},
                                        "name": {"type": "string", "description": "キー名（管理用）"},
                                        "scope": {"type": "string", "enum": ["READ", "READ_WRITE"]},
                                        "environment": {"type": "string", "enum": ["test", "live"]}
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "API キー作成完了（生キーはこの応答でのみ返る）",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "id": {"type": "string", "format": "uuid"},
                                            "api_key": {"type": "string", "description": "生キー（この時のみ）"},
                                            "key_prefix": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        },
                        "400": {"description": "パラメータ不正"},
                        "401": {"description": "認証失敗"},
                        "403": {"description": "権限不足"}
                    }
                }
            },
            "/settings/api-keys/{id}/revoke": {
                "post": {
                    "tags": ["settings"],
                    "summary": "API キー失効",
                    "operationId": "revoke_api_key",
                    "security": [{"CookieAuth": []}],
                    "parameters": [
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string", "format": "uuid"}
                        }
                    ],
                    "responses": {
                        "204": {"description": "キー失効完了"},
                        "404": {"description": "キーが見つからないか既に失効済み"},
                        "401": {"description": "認証失敗"},
                        "403": {"description": "権限不足"}
                    }
                }
            },
            "/documents/search": {
                "get": {
                    "tags": ["documents"],
                    "summary": "帳票横断検索",
                    "operationId": "search_documents",
                    "security": [{"CookieAuth": []}],
                    "parameters": [
                        {
                            "name": "q",
                            "in": "query",
                            "description": "検索クエリ（1文字以上、空白のみはNG）",
                            "required": true,
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "types",
                            "in": "query",
                            "description": "帳票種別（カンマ区切り: received_order,order,invoice,notice。省略時は全種別）",
                            "required": false,
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "description": "最大件数（既定20、最大50）",
                            "required": false,
                            "schema": {"type": "integer", "format": "int32", "default": 20}
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "検索成功",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "array",
                                        "items": {"$ref": "#/components/schemas/DocumentSearchResult"}
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "不正なパラメータ",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/ApiError"}
                                }
                            }
                        },
                        "401": {"description": "認証失敗"},
                        "403": {"description": "権限不足（Admin専用）"},
                        "500": {"description": "サーバーエラー"}
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "NoticeResponse": {
                    "type": "object",
                    "required": ["notice_id", "partner_name", "target_month", "total", "confirmed", "notice_date", "purchase_order_id"],
                    "properties": {
                        "notice_id": {"type": "string"},
                        "partner_name": {"type": "string"},
                        "target_month": {"type": "string", "format": "date"},
                        "total": {"type": "integer", "format": "int32"},
                        "confirmed": {"type": "boolean"},
                        "notice_date": {"type": "string", "format": "date"},
                        "purchase_order_id": {"type": "string"}
                    }
                },
                "InvoiceResponse": {
                    "type": "object",
                    "required": ["id", "invoice_id", "client_name", "subject", "issue_date", "item_count", "status"],
                    "properties": {
                        "id": {"type": "integer", "format": "int64"},
                        "invoice_id": {"type": "string"},
                        "client_name": {"type": "string"},
                        "subject": {"type": "string"},
                        "project_name": {"type": "string", "nullable": true},
                        "issue_date": {"type": "string", "format": "date"},
                        "target_month": {"type": "string", "format": "date", "nullable": true},
                        "due_date": {"type": "string", "format": "date", "nullable": true},
                        "total_amount": {"type": "integer", "format": "int64", "nullable": true},
                        "item_count": {"type": "integer", "format": "int64"},
                        "status": {"type": "string"},
                        "received_order_id": {"type": "integer", "format": "int64", "nullable": true},
                        "client_accepted_at": {"type": "string", "format": "date-time", "nullable": true}
                    }
                },
                "TimesheetListResponse": {
                    "type": "object",
                    "required": ["timesheets", "summary"],
                    "properties": {
                        "timesheets": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/TimesheetRowResponse"}
                        },
                        "summary": {"$ref": "#/components/schemas/TimesheetSummaryResponse"}
                    }
                },
                "TimesheetRowResponse": {
                    "type": "object",
                    "required": ["id", "target_month", "status", "total_hours", "work_days", "contract_info", "engineer_name", "original_filename"],
                    "properties": {
                        "id": {"type": "integer", "format": "int64"},
                        "target_month": {"type": "string", "format": "date"},
                        "status": {"type": "string"},
                        "total_hours": {"type": "number"},
                        "work_days": {"type": "integer", "format": "int32"},
                        "contract_info": {"type": "string"},
                        "engineer_name": {"type": "string"},
                        "original_filename": {"type": "string"}
                    }
                },
                "TimesheetSummaryResponse": {
                    "type": "object",
                    "required": ["total", "pending", "uploaded", "approved"],
                    "properties": {
                        "total": {"type": "integer", "format": "int64"},
                        "pending": {"type": "integer", "format": "int64"},
                        "uploaded": {"type": "integer", "format": "int64"},
                        "approved": {"type": "integer", "format": "int64"}
                    }
                },
                "DocumentSearchResult": {
                    "type": "object",
                    "required": ["type", "id", "number", "title", "counterparty", "status", "url"],
                    "properties": {
                        "type": {"type": "string", "enum": ["received_order", "order", "invoice", "notice"], "description": "帳票種別"},
                        "id": {"type": "string", "description": "ID（受注書ID、発注書ID、請求書番号、支払通知ID）"},
                        "number": {"type": "string", "description": "番号表示"},
                        "title": {"type": "string", "description": "タイトル（案件名、件名等）"},
                        "counterparty": {"type": "string", "description": "取引先名"},
                        "status": {"type": "string", "description": "状態"},
                        "month": {"type": "string", "format": "date", "nullable": true, "description": "対象月（YYYY-MM形式）"},
                        "url": {"type": "string", "description": "詳細URL"}
                    }
                },
                "ApiError": {
                    "type": "object",
                    "required": ["success", "error"],
                    "properties": {
                        "success": {"type": "boolean", "enum": [false]},
                        "error": {"type": "string", "description": "エラーメッセージ"}
                    }
                }
            },
            "securitySchemes": {
                "CookieAuth": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "sophia_jwt",
                    "description": "Admin 向け JWT（HttpOnly Cookie）。実トークンは記載しない。"
                }
            }
        },
        "tags": [
            {
                "name": "notices",
                "description": "支払通知書管理 API"
            },
            {
                "name": "invoices",
                "description": "請求書管理 API"
            },
            {
                "name": "timesheets",
                "description": "稼働報告管理 API"
            },
            {
                "name": "documents",
                "description": "帳票横断検索 API"
            },
            {
                "name": "settings",
                "description": "設定・管理 API"
            }
        ]
    });
    // json! マクロの再帰上限を超えるため、請求書の確定まわりは別に組み立てて差し込む
    api["paths"]["/billing/preview"] = billing_preview_path();
    api["paths"]["/billing/confirm"] = billing_confirm_path();
    api["paths"]["/assignments"] = assignments_create_path();
    api["paths"]["/assignments/preview"] = assignments_preview_path();
    api["paths"]["/assignments/overview"] = assignments_overview_path();
    if let Some(tags) = api["tags"].as_array_mut() {
        tags.push(json!({"name": "billing", "description": "請求書のプレビューと確定 API"}));
        tags.push(json!({"name": "assignments", "description": "アサイン(受注契約+発注契約)の作成と利益試算 API"}));
    }
    api
}

fn billing_preview_path() -> serde_json::Value {
    json!({
                "get": {
                    "tags": ["billing"],
                    "summary": "請求書のプレビュー(保存しない)",
                    "description": "承認済みの勤務表から、請求書ごとの明細・合計・あと何名(missing)・確定できるか(confirmable)を計算して返す。取引先の請求単位(案件ごと/取引先まとめ)に従う。管理者専用。",
                    "operationId": "billing_preview",
                    "security": [{"CookieAuth": []}],
                    "parameters": [
                        {"name": "month", "in": "query", "required": true, "description": "対象月(YYYY-MM)", "schema": {"type": "string"}},
                        {"name": "client_id", "in": "query", "required": false, "description": "取引先ID(省略時は全取引先)", "schema": {"type": "integer", "format": "int64"}}
                    ],
                    "responses": {
                        "200": {"description": "プレビュー: {month, invoices[{key, client_id, client_name, billing_unit, project_id, subject, items, subtotal, tax_amount, total, missing, already_issued_count, confirmable, edi_excluded}]}"},
                        "400": {"description": "month が不正"},
                        "403": {"description": "権限不足(管理者専用)"}
                    }
                }
            })
}

fn billing_confirm_path() -> serde_json::Value {
    json!({
                "post": {
                    "tags": ["billing"],
                    "summary": "請求書の確定",
                    "description": "取引先×月の排他ロックの中で再計算し、請求書を作る。揃っていない場合は force=true と force_reason(必須)と、遅れている契約ごとの force_actions(NEXT_MONTH=翌月回し / SECOND_INVOICE=当月2通目)で強制確定でき、履歴が残る。請求書を先方が作る取引先（先方の EDI で作成代行）は対象外。管理者専用。",
                    "operationId": "billing_confirm",
                    "security": [{"CookieAuth": []}],
                    "requestBody": {
                        "required": true,
                        "content": {"application/json": {"schema": {
                            "type": "object",
                            "required": ["month", "keys"],
                            "properties": {
                                "month": {"type": "string", "description": "YYYY-MM"},
                                "keys": {"type": "array", "items": {"type": "string"}, "description": "プレビューの行ID(最大50)"},
                                "force": {"type": "boolean", "default": false},
                                "force_reason": {"type": "string", "description": "force=true のとき必須(500文字以内)"},
                                "force_actions": {"type": "array", "items": {"type": "object", "properties": {
                                    "client_contract_id": {"type": "integer", "format": "int64"},
                                    "action": {"type": "string", "enum": ["NEXT_MONTH", "SECOND_INVOICE"]}
                                }}}
                            }
                        }}}
                    },
                    "responses": {
                        "200": {"description": "確定: {success, created[{key, invoice_id, invoice_no, total}], skipped[{key, reason, missing}]}"},
                        "400": {"description": "入力が不正(理由なしの強制確定、遅れている契約の扱いが未指定など)"},
                        "403": {"description": "権限不足(管理者専用)"},
                        "409": {"description": "勤務表が揃っていない/確定できる請求書がない(skipped に詳細)"}
                    }
                }
            })
}

fn assignments_create_path() -> serde_json::Value {
    json!({
        "post": {
            "tags": ["assignments"],
            "summary": "アサイン作成",
            "description": "既存の案件に要員をアサインする。受注契約は常に、発注契約(提案元パートナー)はパートナー要員(staff_type=PARTNER)のときだけ作る。自社社員(EMPLOYEE)に partner_contract を渡すと 400。1回で作り、途中で失敗したら全部取り消す。同じ要員・同じ開始日の受注契約が既にあれば 409。管理者専用。",
            "operationId": "create_assignment",
            "security": [{"CookieAuth": []}],
            "requestBody": {
                "required": true,
                "content": {"application/json": {"schema": {
                    "type": "object",
                    "required": ["project_id", "staff_type", "client_contract"],
                    "properties": {
                        "project_id": {"type": "string"},
                        "staff_type": {"type": "string", "enum": ["EMPLOYEE", "PARTNER"]},
                        "engineer_id": {"type": "integer", "format": "int64", "description": "既存の要員(new_engineer とどちらか一方)"},
                        "new_engineer": {"type": "object", "properties": {"name": {"type": "string"}, "name_kana": {"type": "string"}, "email": {"type": "string"}}},
                        "client_contract": {"type": "object", "description": "start_date, end_date, base_rate, effort(既定1), lower_limit_hours, upper_limit_hours, fixed_hours, deduction_rate, overtime_rate, settlement_type(既定=上下割), remarks"},
                        "partner_contract": {"type": "object", "description": "partner_id(提案元パートナー)と、client_contract と同じ項目 + work_location_name"}
                    }
                }}}
            },
            "responses": {
                "201": {"description": "作成: {success, engineer_id, client_contract_id, partner_contract_id|null}"},
                "400": {"description": "入力が不正(区分と発注契約の食い違い、日付・単価・精算幅など)"},
                "403": {"description": "権限不足(管理者専用)"},
                "404": {"description": "案件・提案元パートナー・要員が見つからない"},
                "409": {"description": "同じ要員・同じ開始日の受注契約が既にある"}
            }
        }
    })
}

fn assignments_preview_path() -> serde_json::Value {
    json!({
        "post": {
            "tags": ["assignments"],
            "summary": "アサインの利益試算(保存しない)",
            "description": "受注の単価・精算幅と、発注(パートナー要員のみ)から、月額の売上・原価・粗利・粗利率、稼働時間別の試算、案件の商流ごとの利益の目安との差、警告(赤字・目安未達・単価の桁・前回単価との差・精算幅のずれ)、案件全体の変化を返す。警告はアサインの作成を妨げない。管理者専用。",
            "operationId": "preview_assignment",
            "security": [{"CookieAuth": []}],
            "requestBody": {
                "required": true,
                "content": {"application/json": {"schema": {
                    "type": "object",
                    "required": ["project_id", "client_contract"],
                    "properties": {
                        "project_id": {"type": "string"},
                        "engineer_id": {"type": "integer", "format": "int64", "description": "既存の要員(前回単価との比較に使う)"},
                        "client_contract": {"type": "object", "description": "base_rate, effort(既定1), lower_limit_hours, upper_limit_hours, fixed_hours, deduction_rate, overtime_rate"},
                        "partner_contract": {"type": "object", "description": "同じ項目。自社社員は省略(原価が分からないため粗利は出ない)"}
                    }
                }}}
            },
            "responses": {
                "200": {"description": "試算: {monthly, target{flow,target_pct,diff_pct,status}, hours_scenarios, warnings[{code,message}], project_total{before,after}}"},
                "400": {"description": "単価・精算幅が不正"},
                "403": {"description": "権限不足(管理者専用)"},
                "404": {"description": "案件が見つからない"}
            }
        }
    })
}

fn assignments_overview_path() -> serde_json::Value {
    json!({
        "get": {
            "tags": ["assignments"],
            "summary": "アサイン編成画面の案件一覧(利益つき)",
            "description": "有効な案件(テスト案件を除く)ごとに、有効な契約の月額の売上・原価・粗利・粗利率、商流ごとの目安との差(status: OK/BELOW/UNSET/NOT_APPLICABLE)、アサイン数、うち自社社員の数(自社社員は原価が分からず、粗利に原価が含まれない)を返す。管理者専用。",
            "operationId": "assignments_overview",
            "security": [{"CookieAuth": []}],
            "responses": {
                "200": {"description": "{projects[{project_id, project_name, client_id, client_name, commercial_flow, assignment_count, internal_count, revenue, cost, gross_profit, margin_pct, target_pct, diff_pct, status}], targets{direct, subcontract}}"},
                "403": {"description": "権限不足(管理者専用)"}
            }
        }
    })
}

