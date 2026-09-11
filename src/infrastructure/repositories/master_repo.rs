/// infrastructure/repositories/master_repo.rs — マスタメンテナンス（メタデータ駆動）のDBアクセス
///
/// テーブル定義（`MASTER_TABLES`）をメタデータとして宣言し、CRUD処理は共通エンジンで実行する。
/// fk_table/fk_value/fk_label/table/order_by等はColDef（静的定義、ユーザー入力ではない）由来のため
/// SQL文字列への直接埋め込みで問題ない。
///
/// 新しいマスタを追加する場合は `MASTER_TABLES` に定義を追加するだけでよい。

use sqlx::PgPool;

// ══════════════════════════════════════════════════════════════
//  メタデータ定義 — マスタテーブルの構成情報
// ══════════════════════════════════════════════════════════════

/// カラム定義
pub struct ColDef {
    /// DB上のカラム名
    pub name: &'static str,
    /// 画面表示ラベル
    pub label: &'static str,
    /// 一覧テーブルに表示するか
    pub in_list: bool,
    /// 編集フォームに表示するか
    pub in_form: bool,
    /// PKカラムか（更新時のWHERE条件）
    pub is_pk: bool,
    /// フォームのinput type（text/email/select/checkbox/number/fk_select）
    pub input_type: &'static str,
    /// select の場合の選択肢 (value, label)
    pub options: &'static [(&'static str, &'static str)],
    /// 新規作成時に必須か
    pub required: bool,
    /// FK参照: 参照先テーブル名（空ならFKではない）
    pub fk_table: &'static str,
    /// FK参照: 値カラム（selectのvalue）
    pub fk_value: &'static str,
    /// FK参照: 表示カラム（selectのlabel）
    pub fk_label: &'static str,
}

/// マスタテーブル定義
pub struct MasterTableDef {
    /// APIパス名（/api/masters/{key}）
    pub key: &'static str,
    /// 画面表示名
    pub label: &'static str,
    /// 画面上でのグループ分け（マスタメンテ画面のカテゴリタブ）
    pub category: &'static str,
    /// DBテーブル名
    pub table: &'static str,
    /// カラム定義
    pub columns: &'static [ColDef],
    /// 一覧のORDER BY
    pub order_by: &'static str,
    /// 一覧SQLのJOIN句（空文字なら不要）
    pub list_join: &'static str,
    /// 一覧SQLのSELECT句の追加カラム（JOIN先）
    pub list_extra_select: &'static str,
    /// 自動採番: プレフィックス（"PRJ"→PRJ00000003, ""→連番なし）
    pub auto_id_prefix: &'static str,
    /// 自動採番: 対象カラム名（空なら自動採番なし）
    pub auto_id_column: &'static str,
    /// 自動採番: 桁数（プレフィックス除く）
    pub auto_id_digits: usize,
}

// ── EDI方式の選択肢 ──
static EDI_OPTIONS: &[(&str, &str)] = &[
    ("", "なし"),
    ("EDI_OASIS", "EDI-OASIS"),
    ("EMAIL", "メール"),
];

// ── 雇用形態の選択肢 ──
static EMPLOYMENT_OPTIONS: &[(&str, &str)] = &[
    ("FULL_TIME", "正社員"),
    ("CONTRACT", "契約社員"),
    ("PART_TIME", "パート"),
];

// ── 口座種類の選択肢 ──
static ACCOUNT_OPTIONS: &[(&str, &str)] = &[
    ("普通", "普通"),
    ("当座", "当座"),
];

// ── 精算方式の選択肢 ──
static SETTLEMENT_OPTIONS: &[(&str, &str)] = &[
    ("RANGE", "上下割"),
    ("RANGE_MIDDLE", "中間割"),
    ("RANGE_FIXED_RATE", "一律割"),
    ("FIXED", "固定"),
];

// ── 月中ルールの選択肢 ──
static MID_MONTH_OPTIONS: &[(&str, &str)] = &[
    ("FIXED_HOURS", "固定時間制"),
    ("PRORATED", "日割り計算"),
    ("FULL", "フル（精算幅そのまま）"),
    ("HALF", "半月"),
];

// ── 所属区分の選択肢 ──
static AFFILIATION_OPTIONS: &[(&str, &str)] = &[
    ("PARTNER", "パートナー"),
    ("EMPLOYEE", "自社社員"),
];

// ── 締め日の選択肢 ──
static CLOSING_DAY_OPTIONS: &[(&str, &str)] = &[
    ("15", "15日締め"),
    ("20", "20日締め"),
    ("25", "25日締め"),
    ("0", "月末締め"),
];

// ── 支払月の選択肢 ──
static PAYMENT_MONTH_OPTIONS: &[(&str, &str)] = &[
    ("0", "当月"),
    ("1", "翌月"),
    ("2", "翌々月"),
];

// ── 支払日の選択肢 ──
static PAYMENT_DAY_OPTIONS: &[(&str, &str)] = &[
    ("10", "10日払い"),
    ("15", "15日払い"),
    ("20", "20日払い"),
    ("25", "25日払い"),
    ("0", "末日払い"),
];

// ══════════════════════════════════════════════════════════════
//  マスタテーブル一覧 — 追加はここだけ！
// ══════════════════════════════════════════════════════════════

pub static MASTER_TABLES: &[MasterTableDef] = &[
    MasterTableDef {
        key: "clients",
        label: "クライアント",
        category: "取引先・案件",
        table: "m_client",
        order_by: "id",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: true, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "name", label: "名前", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "contact_person", label: "担当者", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "phone", label: "電話", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "address", label: "住所", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "edi_system_type", label: "EDI方式", in_list: true, in_form: true, is_pk: false, input_type: "select", options: EDI_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "edi_notification_email", label: "EDI通知メール", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "work_report_email", label: "稼働報告送付先", in_list: false, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "invoice_email", label: "請求書送付先（宛先）", in_list: false, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "cc_email", label: "請求書送付先 CC（カンマ区切り）", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "registration_no", label: "適格請求書発行事業者登録番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "peppol_participant_id", label: "Peppol参加者ID", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "partners",
        label: "パートナー",
        category: "取引先・案件",
        table: "m_partner",
        order_by: "partner_id",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "partner_id", auto_id_digits: 10,
        columns: &[
            ColDef { name: "partner_id", label: "パートナーID", in_list: true, in_form: false, is_pk: true, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "name", label: "名前", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "representative_name", label: "代表者", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "cc", label: "CC（カンマ区切り）", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "tel", label: "電話", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "address", label: "住所", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "postal_code", label: "郵便番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "contact_person", label: "連絡担当", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "registration_no", label: "登録番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "bank_name", label: "銀行名", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "bank_branch", label: "支店名", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "account_type", label: "口座種類", in_list: false, in_form: true, is_pk: false, input_type: "select", options: ACCOUNT_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "account_number", label: "口座番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "peppol_participant_id", label: "Peppol参加者ID", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // "projects"(案件)エントリはWS3(2026-07-31)で削除。専用ページ /projects + 作成ウィザードに移行済み
    // (presentation/handlers/projects.rs, infrastructure/repositories/project_repo.rs)。
    // fk_table: "m_project" を参照する他エントリ("partner_contracts"のproject_id等)は
    // fetch_fk_options() が動的にテーブル名でSELECTするため、このエントリ削除の影響を受けない。
    MasterTableDef {
        key: "employees",
        label: "社員",
        category: "人員・勤務地",
        table: "m_employee",
        order_by: "employee_id",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "employee_id", auto_id_digits: 4,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "employee_id", label: "社員番号", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "last_name", label: "姓", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "first_name", label: "名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "employment_type", label: "雇用形態", in_list: true, in_form: true, is_pk: false, input_type: "select", options: EMPLOYMENT_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "base_salary", label: "基本給", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // ── 給与関連マスタ ──
    MasterTableDef {
        key: "bank_master",
        label: "銀行",
        category: "給与・税務",
        table: "m_bank_master",
        order_by: "bank_code, branch_code",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "bank_code", label: "銀行コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "bank_name", label: "銀行名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "branch_code", label: "支店コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "branch_name", label: "支店名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "payment_terms",
        label: "支払条件",
        category: "給与・税務",
        table: "m_payment_term",
        order_by: "m_payment_term.id",
        list_join: " LEFT JOIN m_partner p ON m_payment_term.partner_id = p.partner_id",
        list_extra_select: ", p.name as partner_name",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name" },
            ColDef { name: "partner_name", label: "パートナー", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "closing_day", label: "締め日", in_list: true, in_form: true, is_pk: false, input_type: "select", options: CLOSING_DAY_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "payment_month_offset", label: "支払月", in_list: true, in_form: true, is_pk: false, input_type: "select", options: PAYMENT_MONTH_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "payment_day", label: "支払日", in_list: true, in_form: true, is_pk: false, input_type: "select", options: PAYMENT_DAY_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "partner_contracts",
        label: "契約条件",
        category: "取引先・案件",
        table: "m_partner_contract",
        order_by: "m_partner_contract.id DESC",
        list_join: " LEFT JOIN m_partner pa ON m_partner_contract.partner_id = pa.partner_id LEFT JOIN m_project pr ON m_partner_contract.project_id = pr.project_id LEFT JOIN m_engineer e ON m_partner_contract.engineer_id = e.id",
        list_extra_select: ", pa.name as partner_name, pr.name as project_name, COALESCE(e.name, '') as engineer_name",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name" },
            ColDef { name: "partner_name", label: "パートナー", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "project_id", label: "案件", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_project", fk_value: "project_id", fk_label: "name" },
            ColDef { name: "project_name", label: "案件", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "engineer_id", label: "技術者", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_engineer", fk_value: "id", fk_label: "name" },
            ColDef { name: "engineer_name", label: "技術者", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "start_date", label: "開始日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "end_date", label: "終了日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "settlement_type", label: "精算方式", in_list: true, in_form: true, is_pk: false, input_type: "select", options: SETTLEMENT_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "lower_limit_hours", label: "下限H", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "upper_limit_hours", label: "上限H", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "base_rate", label: "単価", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "deduction_rate", label: "控除単価", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "overtime_rate", label: "超過単価", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "mid_month_rule", label: "月中ルール", in_list: false, in_form: true, is_pk: false, input_type: "select", options: MID_MONTH_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "contract_items", label: "契約条件", in_list: false, in_form: true, is_pk: false, input_type: "textarea", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "work_location", label: "作業場所", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: false, fk_table: "m_work_location", fk_value: "name", fk_label: "name" },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // "work_locations"(作業場所) / "workplaces"(勤務地) エントリは2026-07-31で削除。
    // 案件・契約ごとに決まるものであり汎用マスタとして管理しない方針。
    // 入力は案件作成ウィザード(SelectOrCreate)およびパートナー契約フォームの
    // work_location 欄で行う。form-data API (order_repo::list_work_location_*) と
    // fk_table: "m_work_location" 参照はテーブル直接SELECTのため本削除の影響なし。
    // ── 技術者マスタ ──
    MasterTableDef {
        key: "engineers",
        label: "技術者",
        category: "人員・勤務地",
        table: "m_engineer",
        order_by: "name",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "name", label: "氏名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "name_kana", label: "カナ", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "affiliation_type", label: "所属", in_list: true, in_form: true, is_pk: false, input_type: "select", options: AFFILIATION_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: false, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name" },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "insurance_rates",
        label: "社保料率",
        category: "給与・税務",
        table: "m_insurance_rate",
        order_by: "fiscal_year DESC",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "pension_rate", label: "厚生年金(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "health_rate", label: "健保(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "nursing_rate", label: "介護(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "employment_rate_employee", label: "雇用保険(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "withholding_tax",
        label: "源泉徴収",
        category: "給与・税務",
        table: "m_withholding_tax",
        order_by: "fiscal_year DESC, salary_from",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "salary_from", label: "給与下限", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "salary_to", label: "給与上限", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "tax_dep_0", label: "扶養0人", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "tax_dep_1", label: "扶養1人", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "tax_dep_2", label: "扶養2人", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    MasterTableDef {
        key: "resident_tax",
        label: "住民税",
        category: "給与・税務",
        table: "m_resident_tax_schedule",
        order_by: "fiscal_year DESC, employee_id",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "employee_id", label: "社員", in_list: true, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_employee", fk_value: "id", fk_label: "last_name || ' ' || first_name" },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "municipality", label: "自治体", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_06", label: "6月", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_07", label: "7月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_08", label: "8月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_09", label: "9月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_10", label: "10月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_11", label: "11月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_12", label: "12月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_01", label: "1月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_02", label: "2月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_03", label: "3月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_04", label: "4月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "month_05", label: "5月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // ── 消費税率 ──
    MasterTableDef {
        key: "tax_rate",
        label: "消費税率",
        category: "給与・税務",
        table: "m_tax_rate",
        order_by: "effective_from DESC",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "",
        auto_id_column: "",
        auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "rate", label: "税率(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "effective_from", label: "適用開始日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "effective_to", label: "適用終了日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // ── 経費科目マスタ ──
    MasterTableDef {
        key: "expense_categories",
        label: "経費科目",
        category: "経理・システム",
        table: "m_expense_category",
        order_by: "sort_order",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "code", label: "コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "name", label: "科目名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "sort_order", label: "表示順", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
    // ── メールテンプレート ──
    MasterTableDef {
        key: "email_template",
        label: "メールテンプレート",
        category: "経理・システム",
        table: "s_email_template",
        order_by: "id",
        list_join: "",
        list_extra_select: "",
        auto_id_prefix: "",
        auto_id_column: "",
        auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "code", label: "コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "subject", label: "件名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "body", label: "本文", in_list: false, in_form: true, is_pk: false, input_type: "textarea", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "" },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "" },
        ],
    },
];

// ══════════════════════════════════════════════════════════════
//  DBアクセス関数 — メタデータから動的にSQL生成・実行
// ══════════════════════════════════════════════════════════════

pub fn find_table(key: &str) -> Option<&'static MasterTableDef> {
    MASTER_TABLES.iter().find(|t| t.key == key)
}

/// 指定マスタのフォームが持つ全fk_selectカラムについて、参照先テーブルから
/// (value, label) の選択肢一覧をまとめて取得する。
pub async fn fetch_fk_options(pool: &PgPool, def: &MasterTableDef) -> Vec<(&'static str, Vec<(String, String)>)> {
    let mut result = Vec::new();
    for col in def.columns.iter().filter(|c| c.input_type == "fk_select" && !c.fk_table.is_empty()) {
        let sql = format!(
            "SELECT {}::text AS value, {} AS label FROM {} ORDER BY {}",
            col.fk_value, col.fk_label, col.fk_table, col.fk_label
        );
        let rows: Vec<(String, String)> = sqlx::query_as(&sql)
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("masters: fk-options fetch failed for {}: {:?}", col.fk_table, e);
                vec![]
            });
        result.push((col.name, rows));
    }
    result
}

/// 一覧取得（PgRow をそのまま返す。呼び出し元でJSON変換する）
pub async fn fetch_list(pool: &PgPool, def: &MasterTableDef) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    // SELECT句: 一覧カラム + PKカラム
    let mut select_cols: Vec<String> = Vec::new();

    // list_extra_selectのalias名を抽出（仮想カラム判定用）
    let extra_aliases: Vec<&str> = if !def.list_extra_select.is_empty() {
        def.list_extra_select.split(',')
            .filter_map(|part| part.trim().split(" as ").nth(1).map(|a| a.trim()))
            .collect()
    } else {
        vec![]
    };

    // PKは必ず含める
    for c in def.columns.iter().filter(|c| c.is_pk) {
        select_cols.push(format!("{}.{}", def.table, c.name));
    }
    for c in def.columns.iter().filter(|c| c.in_list && !c.is_pk) {
        // JOINで取得する仮想カラムはスキップ（list_extra_selectで取得）
        if extra_aliases.contains(&c.name) { continue; }
        select_cols.push(format!("{}.{}", def.table, c.name));
    }

    let order_clause = if def.order_by.contains('.') {
        def.order_by.to_string()
    } else {
        format!("{}.{}", def.table, def.order_by)
    };

    let sql = format!(
        "SELECT {}{} FROM {}{} ORDER BY {} LIMIT 200",
        select_cols.join(", "),
        def.list_extra_select,
        def.table,
        def.list_join,
        order_clause,
    );

    sqlx::query(&sql).fetch_all(pool).await
}

/// 詳細取得（PgRow をそのまま返す）
pub async fn fetch_detail(pool: &PgPool, def: &MasterTableDef, id: &str, pk_col: &ColDef, all_cols: &[&str]) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {} FROM {} WHERE {} = $1",
        all_cols.join(", "),
        def.table,
        pk_col.name,
    );

    bind_pk(sqlx::query(&sql), id, pk_col)
        .fetch_optional(pool)
        .await
}

/// 更新実行
pub async fn execute_update(pool: &PgPool, def: &MasterTableDef, id: &str, pk_col: &ColDef, body: &serde_json::Value) -> Result<(), sqlx::Error> {
    let update_cols: Vec<&ColDef> = def.columns.iter()
        .filter(|c| c.in_form && !c.is_pk)
        .collect();

    let set_clause: Vec<String> = update_cols.iter().enumerate()
        .map(|(i, c)| format!("{} = ${}", c.name, i + 1))
        .collect();

    let sql = format!(
        "UPDATE {} SET {} WHERE {} = ${}",
        def.table,
        set_clause.join(", "),
        pk_col.name,
        update_cols.len() + 1,
    );

    let mut query = sqlx::query(&sql);
    for c in &update_cols {
        query = bind_value(query, body, c);
    }
    query = bind_pk(query, id, pk_col);

    query.execute(pool).await?;
    Ok(())
}

/// 新規作成実行。自動採番カラムがあれば採番して body に埋め込んでから挿入する
pub async fn execute_create(pool: &PgPool, def: &MasterTableDef, mut body: serde_json::Value) -> Result<(), sqlx::Error> {
    // ── 自動採番: auto_id_column が設定されていて、値が空ならDBから次番号を生成 ──
    if !def.auto_id_column.is_empty() {
        let current_val = body.get(def.auto_id_column)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if current_val.is_empty() {
            let next_id = generate_next_id(pool, def).await?;
            body[def.auto_id_column] = serde_json::Value::String(next_id);
        }
    }

    let insert_cols: Vec<&ColDef> = def.columns.iter()
        .filter(|c| c.in_form || (c.is_pk && c.input_type == "text") || c.name == def.auto_id_column)
        .collect();

    let col_names: Vec<&str> = insert_cols.iter().map(|c| c.name).collect();
    let placeholders: Vec<String> = (1..=insert_cols.len()).map(|i| format!("${}", i)).collect();

    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        def.table,
        col_names.join(", "),
        placeholders.join(", "),
    );

    let mut query = sqlx::query(&sql);
    for c in &insert_cols {
        query = bind_value(query, &body, c);
    }

    query.execute(pool).await?;
    Ok(())
}

/// EDI採番ルールに従ったID自動生成
/// prefix + ゼロ埋め連番（例: PRJ00000003, 0006）
async fn generate_next_id(pool: &PgPool, def: &MasterTableDef) -> Result<String, sqlx::Error> {
    let prefix = def.auto_id_prefix;
    let col = def.auto_id_column;
    let digits = def.auto_id_digits;

    // プレフィックスがある場合: MAX値からプレフィックス部分を除いて数値化
    let sql = if prefix.is_empty() {
        format!(
            "SELECT COALESCE(MAX(CAST({} AS BIGINT)), 0) + 1 AS next_val FROM {}",
            col, def.table
        )
    } else {
        format!(
            "SELECT COALESCE(MAX(CAST(SUBSTRING({} FROM {}) AS BIGINT)), 0) + 1 AS next_val FROM {} WHERE {} LIKE '{}%'",
            col, prefix.len() + 1, def.table, col, prefix
        )
    };

    let row = sqlx::query(&sql).fetch_one(pool).await?;
    let next_val: i64 = sqlx::Row::get(&row, "next_val");

    if prefix.is_empty() {
        Ok(format!("{:0>width$}", next_val, width = digits))
    } else {
        Ok(format!("{}{:0>width$}", prefix, next_val, width = digits))
    }
}

/// 削除実行
pub async fn execute_delete(pool: &PgPool, def: &MasterTableDef, id: &str, pk_col: &ColDef) -> Result<(), sqlx::Error> {
    let sql = format!("DELETE FROM {} WHERE {} = $1", def.table, pk_col.name);
    bind_pk(sqlx::query(&sql), id, pk_col).execute(pool).await?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════
//  ユーティリティ
// ══════════════════════════════════════════════════════════════

/// PKの型に応じてbind（number→i64, text→String）
fn bind_pk<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    id: &'q str,
    pk_col: &ColDef,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    if pk_col.input_type == "number" {
        let id_val = id.parse::<i64>().unwrap_or(0);
        query.bind(id_val)
    } else {
        query.bind(id)
    }
}

/// sqlx::Row から値を取得（型を自動推定）
pub fn try_get_value(row: &sqlx::postgres::PgRow, col: &str) -> serde_json::Value {
    use sqlx::Row;
    // String
    if let Ok(v) = row.try_get::<String, _>(col) {
        return serde_json::Value::String(v);
    }
    // Option<String>
    if let Ok(v) = row.try_get::<Option<String>, _>(col) {
        return match v {
            Some(s) => serde_json::Value::String(s),
            None => serde_json::Value::Null,
        };
    }
    // i64
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return serde_json::json!(v);
    }
    // i32
    if let Ok(v) = row.try_get::<i32, _>(col) {
        return serde_json::json!(v);
    }
    // bool
    if let Ok(v) = row.try_get::<bool, _>(col) {
        return serde_json::json!(v);
    }
    // f64 (DECIMAL)
    if let Ok(v) = row.try_get::<rust_decimal::Decimal, _>(col) {
        return serde_json::json!(v.to_string());
    }
    // DATE
    if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(col) {
        return serde_json::json!(v.to_string());
    }
    // Option<DATE>
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(col) {
        return match v {
            Some(d) => serde_json::json!(d.to_string()),
            None => serde_json::Value::Null,
        };
    }
    serde_json::Value::Null
}

/// JSON値をsqlxクエリにバインド
fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    body: &'q serde_json::Value,
    col: &ColDef,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    match col.input_type {
        "checkbox" => {
            let v = body[col.name].as_bool().unwrap_or(false);
            query.bind(v)
        }
        "date" => {
            let raw = body[col.name].as_str().unwrap_or("");
            if raw.is_empty() {
                query.bind(None::<chrono::NaiveDate>)
            } else {
                let d = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                    .or_else(|_| chrono::NaiveDate::parse_from_str(raw, "%Y/%m/%d"))
                    .ok();
                query.bind(d)
            }
        }
        "number" => {
            // JSON数値（140 等）→ as_f64() で取得
            // JSON文字列（"140" 等）→ as_str() でパース
            // null / 空文字 → None をbind
            if body[col.name].is_null() {
                query.bind(None::<f64>)
            } else if let Some(v) = body[col.name].as_f64() {
                query.bind(v)
            } else {
                let raw = body[col.name].as_str().unwrap_or("");
                if raw.is_empty() {
                    query.bind(None::<f64>)
                } else {
                    let v = raw.parse::<f64>().unwrap_or(0.0);
                    query.bind(v)
                }
            }
        }
        "fk_select" => {
            // FK参照: fk_valueが"id"ならBIGINT、それ以外はTEXT
            // 空文字列・null はNULLをbind（FK制約違反防止）
            if col.fk_value == "id" {
                if let Some(v) = body[col.name].as_i64().filter(|v| *v > 0) {
                    query.bind(Some(v))
                } else {
                    let s = body[col.name].as_str().unwrap_or("");
                    if s.is_empty() || s == "0" {
                        query.bind(None::<i64>)
                    } else {
                        let v = s.parse::<i64>().unwrap_or(0);
                        if v == 0 { query.bind(None::<i64>) } else { query.bind(Some(v)) }
                    }
                }
            } else {
                let v = body[col.name].as_str().unwrap_or("");
                if v.is_empty() {
                    // required=falseの疑似FK（例: work_location）はDB側がNOT NULL DEFAULT ''の
                    // 単なるVARCHARで実FK制約を持たないため、NULLではなく空文字をbindする
                    // （NULLをbindするとNOT NULL制約違反で更新自体が失敗していた）。
                    // required=trueの実FK列（partner_id/project_id等）は従来通りNULLをbindする
                    // （本来フロント側で空欄を防いでいるはずの異常系のフォールバック）
                    if col.required {
                        query.bind(None::<String>)
                    } else {
                        query.bind(Some(String::new()))
                    }
                } else {
                    query.bind(Some(v.to_string()))
                }
            }
        }
        "select" => {
            // selectの値: JSON数値 or JSON文字列に対応
            // DBカラムが integer の場合もあるため、i64を先に試す
            if let Some(v) = body[col.name].as_i64() {
                query.bind(v)
            } else {
                let raw = body[col.name].as_str().unwrap_or("");
                if raw.is_empty() {
                    query.bind(raw.to_string())
                } else if let Ok(v) = raw.parse::<i64>() {
                    query.bind(v)
                } else {
                    query.bind(raw.to_string())
                }
            }
        }
        _ => {
            let v = body[col.name].as_str().unwrap_or("");
            query.bind(v.to_string())
        }
    }
}
