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
    json!({
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
                "name": "settings",
                "description": "設定・管理 API"
            }
        ]
    })
}
