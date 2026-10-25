/// 組織・ロール・割り当ての管理API(Admin専用。DEMO-000173 PR 5)の OpenAPI パス定義。
///
/// 社内向け OpenAPI は手書きの JSON で持つ。ここは `openapi.rs` の肥大化を避けるための分割。
use serde_json::{json, Value};

/// 1操作の定義を作る。
fn op(tag: &str, id: &str, summary: &str, description: &str, params: Value, body: Option<Value>, ok: (&str, &str), errors: &[(&str, &str)]) -> Value {
    let mut responses = serde_json::Map::new();
    responses.insert(ok.0.to_string(), json!({"description": ok.1}));
    for (code, d) in errors {
        responses.insert((*code).to_string(), json!({"description": d}));
    }
    responses.insert("403".to_string(), json!({"description": "権限不足(管理者専用)"}));
    let mut o = json!({
        "tags": [tag], "summary": summary, "description": description, "operationId": id,
        "security": [{"CookieAuth": []}], "parameters": params, "responses": responses,
    });
    if let Some(b) = body {
        o["requestBody"] = json!({"required": true, "content": {"application/json": {"schema": b}}});
    }
    o
}

fn id_param() -> Value {
    json!([{"name": "id", "in": "path", "required": true, "schema": {"type": "integer", "format": "int64"}}])
}

fn obj(required: &[&str], props: Value) -> Value {
    json!({"type": "object", "required": required, "properties": props})
}

/// 追加するパス(パス文字列 → 操作)。
pub fn paths() -> Vec<(&'static str, Value)> {
    let int = json!({"type": "integer", "format": "int64"});
    let none = json!([]);
    let bad = ("400", "入力が不正");
    let conflict = ("409", "重複・使用中など");
    let nf = ("404", "対象が見つからない");
    vec![
        ("/admin/permissions", json!({"get": op("org-admin", "admin_list_permissions", "権限カタログ",
            "権限コード・日本語名・ドメイン・スタンプ型か・機微か・選べる範囲(スタンプ型は4段階、それ以外は GLOBAL のみ)を返す。",
            none.clone(), None, ("200", "{permissions[{code, label_ja, domain, is_stamped, is_sensitive, allowed_scopes}]}"), &[])})),
        ("/admin/org-units", json!({
            "get": op("org-admin", "admin_list_org_units", "組織の一覧", "無効な組織も含めて返す。", none.clone(), None,
                ("200", "{org_units[{id, parent_id, name, unit_kind, sort_order, is_active}]}"), &[]),
            "post": op("org-admin", "admin_create_org_unit", "組織の作成",
                "親の組織は必須(ルートは1つだけ)。階層は最大8段。同じ親の下に有効な同名の組織は作れない(409)。変更履歴を残す。",
                none.clone(), Some(obj(&["parent_id", "name"], json!({"parent_id": int, "name": {"type": "string"}, "unit_kind": {"type": "string", "nullable": true}, "sort_order": {"type": "integer"}}))),
                ("201", "{org_unit}"), &[bad, conflict]),
        })),
        ("/admin/org-units/{id}", json!({"put": op("org-admin", "admin_update_org_unit", "組織の名前・種別・表示順の変更", "sort_order を省略すると今の値のまま。変更履歴を残す。",
            id_param(), Some(obj(&["name"], json!({"name": {"type": "string"}, "unit_kind": {"type": "string", "nullable": true}, "sort_order": {"type": "integer"}}))),
            ("200", "{org_unit}"), &[bad, nf, conflict])})),
        ("/admin/org-units/{id}/move", json!({"post": op("org-admin", "admin_move_org_unit", "組織の付け替え",
            "循環になる付け替え・ルートの移動・深さ8段超過は400。変更履歴を残す。",
            id_param(), Some(obj(&["new_parent_id"], json!({"new_parent_id": int}))), ("200", "{org_unit}"), &[bad, nf, conflict])})),
        ("/admin/org-units/{id}/active", json!({"post": op("org-admin", "admin_set_org_unit_active", "組織の有効/無効",
            "ルート組織は無効にできない(400)。変化がなければ何もせず履歴も残さない。",
            id_param(), Some(obj(&["is_active"], json!({"is_active": {"type": "boolean"}}))), ("200", "{org_unit}"), &[bad, nf, conflict])})),
        ("/admin/roles", json!({
            "get": op("org-admin", "admin_list_roles", "ロールの一覧", "割り当て数つき。", none.clone(), None,
                ("200", "{roles[{id, code, name, is_system, assignment_count}]}"), &[]),
            "post": op("org-admin", "admin_create_role", "ロールの作成", "権限セットは空から始める。コードは英小文字で始まる英小文字・数字・_ (50文字以内)。",
                none.clone(), Some(obj(&["code", "name"], json!({"code": {"type": "string"}, "name": {"type": "string"}}))), ("201", "{role}"), &[bad, conflict]),
        })),
        ("/admin/roles/{id}", json!({
            "put": op("org-admin", "admin_rename_role", "ロールの表示名の変更", "コードは変えられない。標準ロールの表示名は変更できる。",
                id_param(), Some(obj(&["name"], json!({"name": {"type": "string"}}))), ("200", "{role}"), &[bad, nf]),
            "delete": op("org-admin", "admin_delete_role", "ロールの削除", "標準ロールは削除できない(400)。割り当てが残るロールは削除できない(409)。",
                id_param(), None, ("200", "{success}"), &[bad, nf, conflict]),
        })),
        ("/admin/roles/{id}/permissions", json!({
            "get": op("org-admin", "admin_get_role_permissions", "ロールの権限セット", "", id_param(), None, ("200", "{role, permissions[{permission_code, scope}]}"), &[nf]),
            "put": op("org-admin", "admin_replace_role_permissions", "ロールの権限セットの置き換え",
                "既存の割り当ての組織も含めて検証する。スタンプ型の権限に GLOBAL を持つロールはルート組織への割り当てに限る。スタンプ型でない権限は GLOBAL のみ。不明なコード・重複は400。",
                id_param(), Some(obj(&["permissions"], json!({"permissions": {"type": "array", "items": obj(&["permission_code", "scope"],
                    json!({"permission_code": {"type": "string"}, "scope": {"type": "string", "enum": ["PRIVATE", "UNIT", "HIERARCHICAL", "GLOBAL"]}}))}}))),
                ("200", "{role, permissions}"), &[bad, nf]),
        })),
        ("/admin/assignments", json!({
            "get": op("org-admin", "admin_list_assignments", "割り当ての一覧", "ユーザー・組織・ロールの表示名つき。", none.clone(), None,
                ("200", "{assignments[{id, user_id, user_email, username, org_unit_id, org_unit_name, role_id, role_code, role_name, created_at, created_by}]}"), &[]),
            "post": op("org-admin", "admin_create_assignment", "割り当ての作成",
                "有効なユーザー・有効な組織・存在するロールのみ。スタンプ型の権限に GLOBAL を持つロールはルート組織にだけ割り当てられる。",
                none.clone(), Some(obj(&["user_id", "org_unit_id", "role_id"], json!({"user_id": int, "org_unit_id": int, "role_id": int}))),
                ("201", "{assignment}"), &[bad, conflict]),
        })),
        ("/admin/assignments/{id}", json!({"delete": op("org-admin", "admin_delete_assignment", "割り当てを外す", "", id_param(), None, ("200", "{success}"), &[nf])})),
        ("/admin/owner-transfer", json!({"post": op("org-admin", "admin_owner_transfer", "顧客・案件の担当の付け替え",
            "client_id か project_id のどちらか一方を起点に、その行と配下(案件・契約・発注・支払通知・月次タスク・稼働報告・受注・請求書)の担当組織・担当者をまとめて付け替える。引き継ぎ先は組織・担当者の両方が必須で、有効な組織(上位も有効)と有効な社員のみ。変更履歴(owner_transfer)を1行残す。",
            none.clone(), Some(obj(&["to"], json!({"client_id": int, "project_id": {"type": "string"},
                "to": obj(&["org_unit_id", "employee_id"], json!({"org_unit_id": int, "employee_id": int}))}))),
            ("200", "{updated[{table, count}], total}"), &[bad, nf])})),
        ("/admin/owner-transfer/by-employee", json!({"post": op("org-admin", "admin_owner_transfer_by_employee", "社員が担当のデータの付け替え",
            "from_employee_id が担当のデータすべて(顧客・案件・契約・発注・支払通知・月次タスク・稼働報告・受注・請求書)の担当組織・担当者を付け替える(退職時の引き継ぎ)。引き継ぎ先に同じ社員は選べない。変更履歴を1行残す。",
            none.clone(), Some(obj(&["from_employee_id", "to"], json!({"from_employee_id": int,
                "to": obj(&["org_unit_id", "employee_id"], json!({"org_unit_id": int, "employee_id": int}))}))),
            ("200", "{updated[{table, count}], total}"), &[bad, nf])})),
        ("/admin/owner-transfer/by-employee/{employee_id}", json!({"get": op("org-admin", "admin_owned_by_employee", "社員が担当のデータの件数",
            "付け替え画面のプレビュー用。0件のテーブルも含めて返す。",
            json!([{"name": "employee_id", "in": "path", "required": true, "schema": {"type": "integer", "format": "int64"}}]), None,
            ("200", "{owned[{table, count}], by_org[{org_unit_id, org_name, count}], total}"), &[nf])})),
        ("/admin/owner-transfer/current", json!({"get": op("org-admin", "admin_owner_current", "顧客・案件の今の担当",
            "client_id か project_id のどちらか一方。組織名・担当者名つき。未設定なら null。",
            json!([{"name": "client_id", "in": "query", "required": false, "schema": {"type": "integer", "format": "int64"}},
                   {"name": "project_id", "in": "query", "required": false, "schema": {"type": "string"}}]), None,
            ("200", "{org_unit_id, org_name, employee_id, employee_name}"), &[("400", "指定の誤り"), ("404", "見つからない")])})),
        ("/admin/auto-import-receiver", json!({
            "get": op("org-admin", "admin_get_auto_import_receiver", "自動取込の受け皿(D16)",
                "EDI 同期・メール取込で、親の担当が無いときに使う既定の担当組織と担当者。未設定なら null。", json!([]), None,
                ("200", "{org_unit_id, employee_id}"), &[]),
            "put": op("org-admin", "admin_put_auto_import_receiver", "自動取込の受け皿を設定する",
                "有効な組織・有効な社員のみ。両方必須。変更は履歴に残す。", json!([]), Some(obj(&["org_unit_id", "employee_id"], json!({"org_unit_id": int, "employee_id": int}))),
                ("200", "{org_unit_id, employee_id}"), &[])})),
        ("/admin/org-role-changes", json!({"get": op("org-admin", "admin_list_org_role_changes", "組織・ロール・割り当ての変更履歴",
            "新しい順。limit の既定は100、最大500。",
            json!([{"name": "limit", "in": "query", "required": false, "schema": {"type": "integer"}}]), None,
            ("200", "{changes[{id, changed_at, changed_by, changed_by_name, target_kind, target_id, action, before_json, after_json}]}"), &[])})),
    ]
}
