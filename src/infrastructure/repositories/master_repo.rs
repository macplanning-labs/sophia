/// infrastructure/repositories/master_repo.rs — マスタメンテナンス（メタデータ駆動）のDBアクセス
///
/// テーブル定義（`MASTER_TABLES`）をメタデータとして宣言し、CRUD処理は共通エンジンで実行する。
/// fk_table/fk_value/fk_label/table/order_by等はColDef（静的定義、ユーザー入力ではない）由来のため
/// SQL文字列への直接埋め込みで問題ない。
///
/// 新しいマスタを追加する場合は `MASTER_TABLES` に定義を追加するだけでよい。

use sqlx::{PgConnection, PgPool};

use crate::domain::value_objects::account_number::{AccountInputError, AccountNumber};
use crate::infrastructure::data_crypto::{self, DataCrypto};
use crate::infrastructure::repositories::bank_account_repo::{self, AccountError, Target};

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
    /// 暗号化の印(口座番号の列だけ `Some`)。`Some` の列は汎用経路の SQL に入れず、`bank_account_repo` を通して読み書きする。
    /// 対象の表と列は `Target` が決める(例: `m_partner.account_number` は `Target::Partner`)
    pub encrypted: Option<Target>,
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
/// 請求書の単位(m_client.billing_unit)。既定は PROJECT(現行どおり案件ごと)
static BILLING_UNIT_OPTIONS: &[(&str, &str)] = &[
    ("PROJECT", "案件ごと"),
    ("CLIENT", "取引先まとめ"),
];

/// select が空で送られたときの値。billing_unit は DB に CHECK 制約があり空文字を入れられないため、既定の PROJECT にする
/// (フォームを経由しない更新や、未選択の新規登録で、保存が失敗しないように)
fn empty_select_default(col_name: &str) -> &'static str {
    if col_name == "billing_unit" { "PROJECT" } else { "" }
}

// 「EDI方式」の選択肢は、取引先ごとのカスタマイズが足す（crate::custom::EDI_OPTIONS）
static EDI_OPTIONS: &[(&str, &str)] = crate::custom::EDI_OPTIONS;

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
        // 一覧に当社の担当（組織・営業担当者。DEMO-000173 の担当スタンプ）を出す。
        // 別名は「 as 」で切り出すため、式の中にカンマを書かない
        list_join: " LEFT JOIN m_org_unit ou ON m_client.owner_org_unit_id = ou.id LEFT JOIN m_employee oe ON m_client.owner_employee_id = oe.id",
        list_extra_select: ", ou.name as owner_org_name, oe.last_name || ' ' || oe.first_name as owner_employee_name",
        auto_id_prefix: "", auto_id_column: "", auto_id_digits: 0,
        columns: &[
            ColDef { name: "id", label: "ID", in_list: true, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "name", label: "名前", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "contact_person", label: "先方の担当者", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "owner_org_name", label: "当社の担当組織", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "owner_employee_name", label: "当社の営業担当", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "phone", label: "電話", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "address", label: "住所", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "edi_system_type", label: "EDI方式", in_list: true, in_form: true, is_pk: false, input_type: "select", options: EDI_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "invoice_issued_by_client", label: "請求書は先方が作る", in_list: false, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "billing_unit", label: "請求単位", in_list: true, in_form: true, is_pk: false, input_type: "select", options: BILLING_UNIT_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "edi_notification_email", label: "EDI通知メール", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "work_report_email", label: "稼働報告送付先", in_list: false, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "invoice_email", label: "請求書送付先（宛先）", in_list: false, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "cc_email", label: "請求書送付先 CC（カンマ区切り）", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "registration_no", label: "適格請求書発行事業者登録番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "peppol_participant_id", label: "Peppol参加者ID", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "partner_id", label: "パートナーID", in_list: true, in_form: false, is_pk: true, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "name", label: "名前", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "representative_name", label: "代表者", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "cc", label: "CC（カンマ区切り）", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "tel", label: "電話", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "address", label: "住所", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "postal_code", label: "郵便番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "contact_person", label: "先方の担当者", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "registration_no", label: "登録番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "bank_name", label: "銀行名", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "bank_branch", label: "支店名", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "account_type", label: "口座種類", in_list: false, in_form: true, is_pk: false, input_type: "select", options: ACCOUNT_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "account_number", label: "口座番号", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: Some(Target::Partner) },
            ColDef { name: "peppol_participant_id", label: "Peppol参加者ID", in_list: false, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "employee_id", label: "社員番号", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "last_name", label: "姓", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "first_name", label: "名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "employment_type", label: "雇用形態", in_list: true, in_form: true, is_pk: false, input_type: "select", options: EMPLOYMENT_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "base_salary", label: "基本給", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "bank_code", label: "銀行コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "bank_name", label: "銀行名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "branch_code", label: "支店コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "branch_name", label: "支店名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name", encrypted: None },
            ColDef { name: "partner_name", label: "パートナー", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "closing_day", label: "締め日", in_list: true, in_form: true, is_pk: false, input_type: "select", options: CLOSING_DAY_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "payment_month_offset", label: "支払月", in_list: true, in_form: true, is_pk: false, input_type: "select", options: PAYMENT_MONTH_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "payment_day", label: "支払日", in_list: true, in_form: true, is_pk: false, input_type: "select", options: PAYMENT_DAY_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name", encrypted: None },
            ColDef { name: "partner_name", label: "パートナー", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "project_id", label: "案件", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_project", fk_value: "project_id", fk_label: "name", encrypted: None },
            ColDef { name: "project_name", label: "案件", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "engineer_id", label: "技術者", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_engineer", fk_value: "id", fk_label: "name", encrypted: None },
            ColDef { name: "engineer_name", label: "技術者", in_list: true, in_form: false, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "start_date", label: "開始日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "end_date", label: "終了日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "settlement_type", label: "精算方式", in_list: true, in_form: true, is_pk: false, input_type: "select", options: SETTLEMENT_OPTIONS, required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "lower_limit_hours", label: "下限H", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "upper_limit_hours", label: "上限H", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "base_rate", label: "単価", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "deduction_rate", label: "控除単価", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "overtime_rate", label: "超過単価", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "mid_month_rule", label: "月中ルール", in_list: false, in_form: true, is_pk: false, input_type: "select", options: MID_MONTH_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "contract_items", label: "契約条件", in_list: false, in_form: true, is_pk: false, input_type: "textarea", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "work_location", label: "作業場所", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: false, fk_table: "m_work_location", fk_value: "name", fk_label: "name", encrypted: None },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "name", label: "氏名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "name_kana", label: "カナ", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "affiliation_type", label: "所属", in_list: true, in_form: true, is_pk: false, input_type: "select", options: AFFILIATION_OPTIONS, required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "partner_id", label: "パートナー", in_list: false, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: false, fk_table: "m_partner", fk_value: "partner_id", fk_label: "name", encrypted: None },
            ColDef { name: "email", label: "メール", in_list: true, in_form: true, is_pk: false, input_type: "email", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "pension_rate", label: "厚生年金(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "health_rate", label: "健保(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "nursing_rate", label: "介護(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "employment_rate_employee", label: "雇用保険(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "salary_from", label: "給与下限", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "salary_to", label: "給与上限", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "tax_dep_0", label: "扶養0人", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "tax_dep_1", label: "扶養1人", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "tax_dep_2", label: "扶養2人", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "employee_id", label: "社員", in_list: true, in_form: true, is_pk: false, input_type: "fk_select", options: &[], required: true, fk_table: "m_employee", fk_value: "id", fk_label: "last_name || ' ' || first_name", encrypted: None },
            ColDef { name: "fiscal_year", label: "年度", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "municipality", label: "自治体", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_06", label: "6月", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_07", label: "7月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_08", label: "8月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_09", label: "9月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_10", label: "10月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_11", label: "11月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_12", label: "12月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_01", label: "1月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_02", label: "2月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_03", label: "3月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_04", label: "4月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "month_05", label: "5月", in_list: false, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "rate", label: "税率(%)", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "effective_from", label: "適用開始日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "effective_to", label: "適用終了日", in_list: true, in_form: true, is_pk: false, input_type: "date", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "code", label: "コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "name", label: "科目名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "sort_order", label: "表示順", in_list: true, in_form: true, is_pk: false, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "is_active", label: "有効", in_list: true, in_form: true, is_pk: false, input_type: "checkbox", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
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
            ColDef { name: "id", label: "ID", in_list: false, in_form: false, is_pk: true, input_type: "number", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "code", label: "コード", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "subject", label: "件名", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "body", label: "本文", in_list: false, in_form: true, is_pk: false, input_type: "textarea", options: &[], required: true, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
            ColDef { name: "description", label: "説明", in_list: true, in_form: true, is_pk: false, input_type: "text", options: &[], required: false, fk_table: "", fk_value: "", fk_label: "", encrypted: None },
        ],
    },
];

// ══════════════════════════════════════════════════════════════
//  DBアクセス関数 — メタデータから動的にSQL生成・実行
// ══════════════════════════════════════════════════════════════

/// 汎用経路の SQL に載せてよい列(暗号化された列は除く)。SQL を組み立てるときは、必ずここを通す(ガード R11)
fn plain_cols<'a>(cols: impl Iterator<Item = &'a ColDef>) -> impl Iterator<Item = &'a ColDef> {
    cols.filter(|c| c.encrypted.is_none())
}

/// 暗号化された列の値の形式を確かめる。`enc1:` で始まる値は平文として拒否する(値は返さない)。
/// 保存の前にハンドラが呼ぶ(保存の途中で止めて 500 にしないため)。
pub fn check_encrypted_values(def: &MasterTableDef, body: &serde_json::Value) -> Result<(), &'static str> {
    for c in def.columns.iter().filter(|c| c.encrypted.is_some()) {
        match body.get(c.name) {
            // 欄が無い・null は「触らない」(更新では既存の値を残す。新規では空で作る)
            None | Some(serde_json::Value::Null) => {}
            // 文字列以外(数値など)は、空に化けて既存の値を消すのを防ぐため断る
            Some(v) => {
                let s = v.as_str().ok_or(ACCOUNT_TYPE_MESSAGE)?;
                // 入力検証(数字のみ・桁数・全角の直し)は parse_input に集約。理由は固定文言(値は返さない)
                AccountNumber::parse_input(s).map_err(AccountInputError::message)?;
            }
        }
    }
    Ok(())
}

/// 口座番号の型の誤りを、理由の固定文言で返す(入力の値は返さない)。形式の誤りは `AccountInputError::message`
const ACCOUNT_TYPE_MESSAGE: &str = "口座番号は文字列で入力してください";

/// 口座番号の読み書きの失敗を、sqlx のエラーの形に変える(復号の失敗も、空で代用せず Err にする)
fn to_sql_err(e: AccountError) -> sqlx::Error {
    match e {
        AccountError::Db(e) => e,
        AccountError::Crypto(m) => sqlx::Error::Protocol(m),
    }
}

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
    for c in plain_cols(def.columns.iter()).filter(|c| c.is_pk) {
        select_cols.push(format!("{}.{}", def.table, c.name));
    }
    for c in plain_cols(def.columns.iter()).filter(|c| c.in_list && !c.is_pk) {
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

/// 詳細取得: 画面用の値(フォームの列と主キー)。暗号化された列は復号して平文で入れる(画面は従来どおり全て表示)。
/// 行が無ければ Ok(None)。復号できなければ Err(空で代用しない)。
pub async fn fetch_detail_values(pool: &PgPool, def: &MasterTableDef, id: &str) -> Result<Option<serde_json::Map<String, serde_json::Value>>, sqlx::Error> {
    // 暗号化された列を持つ表だけ、暗号化の設定を読む(持たない表は設定が無くても詳細を返す)
    let dc = if has_encrypted(def) {
        Some(data_crypto::global().map_err(sqlx::Error::Protocol)?)
    } else {
        None
    };
    fetch_detail_values_with(dc, pool, def, id).await
}

/// `fetch_detail_values` の本体(暗号化の設定を引数で受ける。試験で鍵なしの場合を確かめるため)。
/// 暗号化された列を持つ表で `dc` が None なら Err(空で代用しない)
pub async fn fetch_detail_values_with(dc: Option<&DataCrypto>, pool: &PgPool, def: &MasterTableDef, id: &str) -> Result<Option<serde_json::Map<String, serde_json::Value>>, sqlx::Error> {
    let pk_col = def.columns.iter().find(|c| c.is_pk).ok_or_else(|| sqlx::Error::Protocol("主キーの列がありません".into()))?;
    let cols: Vec<&str> = plain_cols(def.columns.iter()).filter(|c| c.in_form || c.is_pk).map(|c| c.name).collect();
    let sql = format!("SELECT {} FROM {} WHERE {} = $1", cols.join(", "), def.table, pk_col.name);
    let Some(row) = bind_pk(sqlx::query(&sql), id, pk_col).fetch_optional(pool).await? else {
        return Ok(None);
    };

    let mut conn = pool.acquire().await?;
    let mut obj = serde_json::Map::new();
    for c in def.columns.iter().filter(|c| c.in_form || c.is_pk) {
        let value = match c.encrypted {
            None => try_get_value(&row, c.name),
            Some(target) => {
                let dc = dc.ok_or_else(|| sqlx::Error::Protocol("暗号化の設定がありません".into()))?;
                let account = bank_account_repo::read_account_with(dc, &mut conn, target, id).await.map_err(to_sql_err)?;
                let account = account.ok_or(sqlx::Error::RowNotFound)?;
                serde_json::Value::String(account.expose().to_string())
            }
        };
        obj.insert(c.name.to_string(), value);
    }
    Ok(Some(obj))
}

/// 更新実行
pub async fn execute_update(pool: &PgPool, def: &MasterTableDef, id: &str, pk_col: &ColDef, body: &serde_json::Value) -> Result<(), sqlx::Error> {
    let update_cols: Vec<&ColDef> = plain_cols(def.columns.iter())
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

    let mut tx = pool.begin().await?;
    query.execute(&mut *tx).await?;
    write_encrypted_columns(&mut tx, def, id, body, Absent::Keep).await?;
    tx.commit().await?;
    Ok(())
}

/// 暗号化された列の欄が無い・null のときの扱い
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Absent {
    /// 既存の値を残す(更新。欄が無いのは「触らない」の意味。空文字を送ったときだけ空にする)
    Keep,
    /// 空で書く(新規。従来どおり未登録)
    Empty,
}

/// 暗号化された列を、暗号化して書く(呼び出し元のトランザクションの中で)。対象の行は主キー `key` で指定する
async fn write_encrypted_columns(conn: &mut PgConnection, def: &MasterTableDef, key: &str, body: &serde_json::Value, absent: Absent) -> Result<(), sqlx::Error> {
    for c in def.columns.iter().filter(|c| c.in_form && !c.is_pk) {
        let Some(target) = c.encrypted else { continue };
        let raw = match body.get(c.name) {
            None | Some(serde_json::Value::Null) => match absent {
                Absent::Keep => continue,
                Absent::Empty => "",
            },
            Some(v) => v.as_str().ok_or_else(|| sqlx::Error::Protocol(ACCOUNT_TYPE_MESSAGE.into()))?,
        };
        // 保存の前に検証し、正規化した値(数字だけ)を書く。理由は固定文言で返す(入力の値は返さない)
        let value = AccountNumber::parse_input(raw).map_err(|e| sqlx::Error::Protocol(e.message().into()))?;
        bank_account_repo::write_account_tx(conn, target, key, &value).await.map_err(to_sql_err)?;
    }
    Ok(())
}

/// マスタ画面から顧客(m_client)を作ろうとしたときの案内。担当(組織と担当者)の指定が要るため、専用の画面・APIから作る。
pub const M_CLIENT_CREATE_REJECTED: &str = "取引先は「取引先」画面から登録してください(担当の指定が必要なため)";

/// このマスタ表の新規作成を、マスタ画面では受け付けないときの理由(ハンドラが400で返す)。
pub fn create_rejection(def: &MasterTableDef) -> Option<&'static str> {
    (def.table == "m_client").then_some(M_CLIENT_CREATE_REJECTED)
}

/// 新規作成実行。自動採番カラムがあれば採番して body に埋め込んでから挿入する。
/// 自動採番のIDがある表(例: パートナー)は、作ったIDを返す(画面が、作った直後に選べるように)
pub async fn execute_create(pool: &PgPool, def: &MasterTableDef, mut body: serde_json::Value) -> Result<Option<String>, sqlx::Error> {
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

    // 顧客は担当の指定が要るため、ここでは作らない(ハンドラが先に400で断る。直接呼ばれたときの最後の砦)。
    if def.table == "m_client" {
        return Err(sqlx::Error::Protocol(M_CLIENT_CREATE_REJECTED.to_string()));
    }

    let insert_cols: Vec<&ColDef> = plain_cols(def.columns.iter())
        .filter(|c| c.in_form || (c.is_pk && c.input_type == "text") || c.name == def.auto_id_column)
        .collect();

    let mut col_names: Vec<&str> = insert_cols.iter().map(|c| c.name).collect();
    let mut placeholders: Vec<String> = (1..=insert_cols.len()).map(|i| format!("${}", i)).collect();

    // 担当スタンプ(設計書 §6.6): 発注契約は案件 → 顧客から、案件は顧客から写す。
    // 顧客(m_client)は担当の指定が必須なので、マスタ画面からは作らせない(「取引先」画面の専用API `POST /api/v1/clients` から)。
    // 担当スタンプを持つ表をこのマスタ画面に足したら、ここにも分岐を足す(tests/owner_stamp_guard.rs が検査する)。
    let stamp = if def.table == "m_partner_contract" {
        let project_id = body.get("project_id").and_then(|v| v.as_str()).unwrap_or("");
        let mut conn = pool.acquire().await?;
        Some(super::stamp_repo::for_project_child(&mut conn, project_id, "m_partner_contract", crate::domain::authz::stamp::StampMode::Manual).await?)
    } else if def.table == "m_project" {
        let client_id = body.get("client_id").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())));
        let mut conn = pool.acquire().await?;
        Some(super::stamp_repo::for_project(&mut conn, client_id, crate::domain::authz::stamp::StampMode::Manual).await?)
    } else {
        None
    };
    if stamp.is_some() {
        col_names.push("owner_org_unit_id");
        col_names.push("owner_employee_id");
        placeholders.push(format!("${}", insert_cols.len() + 1));
        placeholders.push(format!("${}", insert_cols.len() + 2));
    }

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
    if let Some(st) = stamp {
        query = query.bind(st.org_unit_id).bind(st.employee_id);
    }

    let mut tx = pool.begin().await?;
    query.execute(&mut *tx).await?;
    if has_encrypted(def) {
        // 暗号化された列の行は、作ったときの主キー(自動採番の ID)で指定する。無ければ書けないため失敗させる
        let key = created_id(def, &body).ok_or_else(|| sqlx::Error::Protocol("暗号化された列を持つ表は主キーの値が必要です".into()))?;
        write_encrypted_columns(&mut tx, def, &key, &body, Absent::Empty).await?;
    }
    tx.commit().await?;
    Ok(created_id(def, &body))
}

/// 暗号化された列を持つ表か(持つ表だけ、暗号化の設定を読む)
fn has_encrypted(def: &MasterTableDef) -> bool {
    def.columns.iter().any(|c| c.encrypted.is_some())
}

/// 作ったIDを body から取り出す(自動採番カラムがある表のみ)
fn created_id(def: &MasterTableDef, body: &serde_json::Value) -> Option<String> {
    if def.auto_id_column.is_empty() {
        return None;
    }
    body.get(def.auto_id_column).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string())
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
                    query.bind(empty_select_default(col.name).to_string())
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

#[cfg(test)]
mod billing_unit_tests {
    use super::*;

    #[test]
    fn empty_billing_unit_falls_back_to_project() {
        assert_eq!(empty_select_default("billing_unit"), "PROJECT");
        assert_eq!(empty_select_default("edi_system_type"), "");
    }

    #[test]
    fn list_extra_select_aliases_are_declared_display_only_columns() {
        // 一覧の追加列(JOIN)は、一覧API と同じく「カンマで区切り、 as の後ろを別名」として読む。
        // 読めた別名の数が「 as 」の数と同じで、どれも「一覧だけに出す列」として宣言されていることを確かめる
        for def in MASTER_TABLES.iter().filter(|d| !d.list_extra_select.is_empty()) {
            let aliases: Vec<&str> = def.list_extra_select.split(',')
                .filter_map(|part| part.trim().split(" as ").nth(1).map(str::trim))
                .collect();
            assert_eq!(aliases.len(), def.list_extra_select.matches(" as ").count(), "{}: 読めない別名がある", def.key);
            for alias in aliases {
                let col = def.columns.iter().find(|c| c.name == alias)
                    .unwrap_or_else(|| panic!("{}: 別名 {alias} が列に無い", def.key));
                assert!(col.in_list && !col.in_form && !col.is_pk, "{}: {alias} は一覧だけの列にする", def.key);
            }
        }
    }

    #[test]
    fn client_list_shows_our_owner() {
        let def = find_table("clients").expect("clients マスタ");
        for name in ["owner_org_name", "owner_employee_name"] {
            assert!(def.list_extra_select.contains(&format!(" as {name}")), "{name} を一覧で取る");
        }
        let label = |n: &str| def.columns.iter().find(|c| c.name == n).map(|c| c.label);
        assert_eq!(label("contact_person"), Some("先方の担当者"));
        assert_eq!(label("owner_employee_name"), Some("当社の営業担当"));
    }

    #[test]
    fn client_master_has_billing_unit_select_with_both_options() {
        let def = find_table("clients").expect("clients マスタ");
        let col = def.columns.iter().find(|c| c.name == "billing_unit").expect("billing_unit");
        assert_eq!(col.input_type, "select");
        let values: Vec<&str> = col.options.iter().map(|(v, _)| *v).collect();
        assert_eq!(values, vec!["PROJECT", "CLIENT"]);
    }
}

#[cfg(test)]
mod created_id_tests {
    use super::*;

    #[test]
    fn auto_numbered_tables_return_the_created_id() {
        let partners = find_table("partners").expect("partners");
        let body = serde_json::json!({"partner_id": "0000000007", "name": "x"});
        assert_eq!(created_id(partners, &body), Some("0000000007".to_string()));
        let empty = serde_json::json!({"name": "x"});
        assert_eq!(created_id(partners, &empty), None, "IDが無ければ None");
    }

    #[test]
    fn tables_without_auto_id_return_none() {
        let engineers = find_table("engineers").expect("engineers");
        assert_eq!(created_id(engineers, &serde_json::json!({"id": 5})), None);
    }
}

#[cfg(test)]
mod encrypted_column_tests {
    use super::*;

    #[test]
    fn every_encrypted_column_is_form_only_and_points_at_its_own_table() {
        let mut found = 0;
        for def in MASTER_TABLES.iter() {
            for c in def.columns.iter().filter(|c| c.encrypted.is_some()) {
                found += 1;
                let target = c.encrypted.expect("印あり");
                assert!(target.label().starts_with(&format!("{}.", def.table)), "{}: {} の印の対象が表と違う", def.key, c.name);
                assert!(c.in_form && !c.in_list && !c.is_pk, "{}: {} は画面の入力だけに出す(一覧・主キーにしない)", def.key, c.name);
            }
        }
        assert_eq!(found, 1, "印の付いた列は口座番号の 1 つ(パートナー)");
    }

    #[test]
    fn plain_columns_never_include_the_encrypted_column() {
        let def = find_table("partners").expect("partners");
        let plain: Vec<&str> = plain_cols(def.columns.iter()).map(|c| c.name).collect();
        assert!(!plain.contains(&"account_number"), "汎用経路の SQL に口座番号を載せない");
        assert!(plain.contains(&"partner_id") && plain.contains(&"bank_name"), "他の列は載る");
    }

    #[test]
    fn encrypted_values_must_not_look_like_ciphertext() {
        let def = find_table("partners").expect("partners");
        assert!(check_encrypted_values(def, &serde_json::json!({"account_number": "0012345"})).is_ok());
        assert!(check_encrypted_values(def, &serde_json::json!({"account_number": ""})).is_ok(), "空は未登録として通す");
        let err = check_encrypted_values(def, &serde_json::json!({"account_number": "enc1:k1:AAAA"}));
        assert_eq!(err, Err("口座番号の形式が正しくありません"));
    }

    #[tokio::test]
    async fn detail_of_a_table_without_encrypted_column_does_not_need_the_crypto_setting() {
        // 口座番号の列が無い表は、暗号化の設定(install)が無くても詳細を取りに行く。設定が無いことを理由に失敗しない。
        // 接続先は届かない所にする(設定の確認で失敗していないことだけを見る)
        let def = find_table("engineers").expect("engineers");
        assert!(!has_encrypted(def), "前提: engineers は暗号化された列を持たない");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(std::time::Duration::from_millis(200))
            .connect_lazy("postgres://sophia:sophia@127.0.0.1:1/none")
            .expect("遅延接続の作成");
        let err = fetch_detail_values(&pool, def, "1").await.expect_err("接続できないので失敗する");
        assert!(!err.to_string().contains("データ暗号化"), "設定が無いことで失敗しない: {err}");
    }

    #[test]
    fn absent_or_null_account_number_is_not_checked_and_non_string_is_refused() {
        let def = find_table("partners").expect("partners");
        assert!(check_encrypted_values(def, &serde_json::json!({"name": "x"})).is_ok(), "欄なしは通す(既存を残す)");
        assert!(check_encrypted_values(def, &serde_json::json!({"account_number": null})).is_ok(), "null は通す(既存を残す)");
        let err = check_encrypted_values(def, &serde_json::json!({"account_number": 1234567}));
        assert_eq!(err, Err("口座番号は文字列で入力してください"), "数値は 400 にする(空に化けて消さない)");
    }
}
