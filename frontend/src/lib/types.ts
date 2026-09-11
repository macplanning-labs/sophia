// lib/types.ts — Sophia SPA 型定義

// ── 月次確定ダッシュボード ──

export interface SettlementRow {
  engineer_id: number;
  engineer_name: string;
  partner_id: string;
  partner_name: string;
  client_name: string;
  client_id: number;
  timesheet_id: number | null;
  total_hours: string | null; // Decimal as string
  timesheet_status: string | null;
  client_contract_id: number;
  billing_base_rate: number;
  billing_settlement_type: string;
  billing_lower_limit: string;
  billing_upper_limit: string;
  billing_fixed_hours: string | null;
  billing_deduction_rate: number;
  billing_overtime_rate: number;
  billing_effort: string;
  partner_contract_id: number | null;
  payment_base_rate: number;
  payment_settlement_type: string;
  payment_lower_limit: string;
  payment_upper_limit: string;
  payment_fixed_hours: string | null;
  payment_deduction_rate: number;
  payment_overtime_rate: number;
  payment_effort: string;
  invoice_issued: boolean | null;
  notice_issued: boolean | null;
  /** 先方EDIシステム種別（EDI_OASIS 等）。請求書発行対象外判定に使用 */
  client_edi_system_type?: string;
  /** 対象月の発注書（準備状況表示用） */
  purchase_order_id?: string | null;
  purchase_order_status?: string | null;
  project_id: string;
  project_name: string;
  engineer_employee_code: string;
}

export interface SettlementViewRow {
  row: SettlementRow;
  billing_amount: number;
  payment_amount: number;
  profit: number;
  profit_rate: number;
  profit_rate_display: string;
}

export interface SettlementSummary {
  total_count: number;
  total_billing: number;
  total_payment: number;
  total_profit: number;
  avg_profit_rate: number;
  avg_profit_rate_display: string;
  invoice_issued_count: number;
  invoice_pending_count: number;
  notice_issued_count: number;
  notice_pending_count: number;
  unapproved_count: number;
}

export interface SettlementApiResponse {
  rows: SettlementViewRow[];
  summary: SettlementSummary;
  current_month: string;
}

export interface MonthOption {
  value: string;
  label: string;
}

export interface ClientOption {
  id: number;
  name: string;
}

export interface PartnerOption {
  partner_id: string;
  name: string;
}

export interface ProjectOption {
  project_id: string;
  name: string;
}

export interface FiltersApiResponse {
  available_months: MonthOption[];
  clients: ClientOption[];
  partners: PartnerOption[];
  projects: ProjectOption[];
}

export interface ActionBlocker {
  subject: string;
  context: string;
  reason: string;
  suggestion: string;
  link_path: string;
  link_label: string;
  code: string;
}

export type NoticeIssueBlocker = ActionBlocker;

export interface ApiResult {
  ok?: boolean;
  error?: string;
  /** 一部のエンドポイントで使用 */
  success?: boolean;
  message?: string;
  issued_ids?: number[];
  /** issue-invoicesで、その場で作成された請求書一覧（確認・送信モーダルの表示に使う） */
  created_invoices?: CreatedInvoice[];
  /** issue-noticesで、その場で作成された支払通知一覧（確認・送信パネルの表示に使う） */
  created_notices?: CreatedNotice[];
  /** 失敗時の行別診断 */
  blockers?: ActionBlocker[];
}

export interface CreatedInvoice {
  id: number;
  invoice_no: string;
  client_id: number;
  client_name: string;
  project_id: string;
  project_name: string;
  count: number;
  total: number;
}

export interface CreatedNotice {
  notice_id: string;
  partner_id: string;
  partner_name: string;
  project_id: string;
  project_name: string;
  count: number;
  total: number;
  needs_approval: boolean;
}

// ── ダッシュボード ──

export interface StepInfo {
  label: string;
  done: boolean;
  active: boolean;
}

export interface ProgressRow {
  entity_name: string;
  project_id: string | null;
  project_name: string;
  engineer_name: string;
  month: string;
  order_id: string;
  /** 発注進捗のパートナー契約ID（受注進捗では null） */
  partner_contract_id: number | null;
  /** 受注進捗のクライアント契約ID（発注進捗では null） */
  client_contract_id: number | null;
  steps: StepInfo[];
  status: string;       // "complete" | "in_progress" | "overdue" | "not_started"
  status_value: string; // 実際のDBステータス
  days_remaining: number;
  action_text: string;
  row_status: string;   // "done" | "overdue" | "active" | "pending"
}

export interface DashboardApiResponse {
  partner_progress: ProgressRow[];
  client_progress: ProgressRow[];
  partner_list: string[];
  month_list: string[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  mail_logs: any[];
}

// ── 通知ベル(WS5) ──

export interface NotificationItem {
  id: string;
  category: string;
  category_label: string;
  title: string;
  message: string;
  severity: string; // "overdue" | "due_soon"
  days_remaining: number;
  href: string;
  project_id: string | null;
  project_name: string;
}

export interface NotificationsResponse {
  count: number;
  overdue_count: number;
  items: NotificationItem[];
}

// ── プロジェクト別ダッシュボード ──

export interface ProjectSummary {
  project_id: string;
  project_name: string;
  client_name: string;
  staff_internal_count: number;
  staff_partner_count: number;
  timesheet_approved_count: number;
  timesheet_total_count: number;
  pending_engineer_names: string[];
  billing_total: number;
  partner_cost_total: number;
  internal_cost_total: number;
  profit: number;
  is_finalized: boolean;
  engineers: SettlementViewRow[];
}

// ── 発注契約 ──

export interface ContractRow {
  id: number;
  partner_name: string;
  project_name: string;
  engineer_name: string;
  settlement_type: string;
  base_rate: number;
  effort: string; // Decimal
  start_date: string;
  end_date: string;
  is_active: boolean;
}

// ── 発注書 ──

export interface OrderRow {
  order_id: string;
  partner_name: string;
  project_name: string;
  engineer_name: string;
  status: string;
  order_date: string;
  work_start: string;
  work_end: string;
  item_count: number;
  total_amount: number | null;
}

// ── 支払通知 ──

export interface NoticeRow {
  notice_id: string;
  partner_name: string;
  target_month: string;
  total: number;
  confirmed: boolean;
  notice_date: string;
  /** 元発注書ID（ダッシュボード帳票モーダルの紐付け用） */
  purchase_order_id?: string;
}

// ── 受注契約 ──

export interface ClientContractRow {
  id: number;
  project_id: string;
  engineer_id: number;
  start_date: string;
  end_date: string;
  settlement_type: string;
  base_rate: number;
  effort: string;
  is_active: boolean;
  project_name: string;
  client_name: string;
  engineer_name: string;
}

// ── 案件 ──

export interface ProjectRow {
  project_id: string;
  name: string;
  client_id: number;
  client_name: string;
  is_active: boolean;
  created_at: string;
}

export interface WizardResult extends ApiResult {
  project_id?: string;
}

// ── 受注書 ──

export interface ReceivedOrderRow {
  id: number;
  received_order_no: string;
  target_month: string;
  project_name: string;
  engineer_name: string;
  status: string;
  client_name: string;
  is_recurring: boolean;
  created_at: string;
  item_count: number;
  total_amount: number | null;
}

// ── 受信メール（メール自動取込パイプライン） ──

export interface ReceivedEmailRow {
  id: number;
  message_id: string;
  from_email: string;
  from_name: string;
  subject: string;
  received_at: string;
  body_text: string;
  status: string;
  error_message: string;
  attachment_filename: string;
  source_type: string;
  retry_count: number;
  needs_manual_review: boolean;
  review_notified_at: string | null;
  drive_link: string;
  parsed_data: unknown;
  created_at: string;
  processed_at: string | null;
}

// ── 自社情報設定 ──

export interface CompanyInfo {
  id: number;
  name: string;
  postal_code: string;
  address: string;
  tel: string;
  fax: string;
  representative_title: string;
  representative_name: string;
  registration_no: string;
  responsible_person: string;
  contact_person: string;
  bank_name: string;
  bank_branch: string;
  account_type: string;
  account_number: string;
  account_name: string;
  stamp_image: string;
  logo_image: string;
  email_host: string;
  email_port: number | null;
  email_use_tls: boolean;
  email_host_user: string;
  default_from_email: string;
  notice_approval_threshold: number | null;
  token_expiry_days: number | null;
  has_smtp_password: boolean;
}

// ── 請求書 ──

export interface InvoiceRow {
  id: number;
  invoice_id: string;
  client_name: string;
  subject: string;
  project_name: string | null;
  issue_date: string;
  due_date: string | null;
  target_month: string | null;
  total_amount: number | null;
  item_count: number;
  status: string;
  /** 元受注書ID（ダッシュボード帳票モーダルの紐付け用） */
  received_order_id?: number | null;
  /** クライアント受領確認日時。未確認ならnull（削除可否の判定に使う） */
  client_accepted_at?: string | null;
}

// ── 稼働報告 ──

export interface TimesheetRow {
  id: number;
  target_month: string;
  status: string;
  total_hours: string;
  work_days: number;
  contract_info: string;
  engineer_name: string;
  original_filename: string;
}

export interface TimesheetSummary {
  total: number;
  pending: number;
  uploaded: number;
  approved: number;
}

export interface TimesheetApiResponse {
  timesheets: TimesheetRow[];
  summary: TimesheetSummary;
}

// ── タスク ──

export interface TaskRow {
  id: number;
  project_id: string;
  engineer_id: number | null;
  work_month: string;
  task_type: string;
  responsible: string;
  deadline: string;
  status: string;
  completed_at: string | null;
  note: string;
}

export interface TaskSummary {
  total: number;
  done: number;
  overdue: number;
  pending: number;
  completion_rate: number;
}

export interface TaskApiResponse {
  tasks: TaskRow[];
  summary: TaskSummary;
}

// ── 社員 ──

export interface EmployeeRow {
  id: number;
  employee_id: string;
  last_name: string;
  first_name: string;
  employment_type: string;
  hire_date: string | null;
  email: string;
  base_salary: number;
  is_active: boolean;
}

// ── 経費 ──
// 1申請(ヘッダー)は複数の明細(ExpenseItem)を持つ（交通費の複数チケット等に対応するため）。

export interface ExpenseRow {
  id: number;
  status: string;
  total_amount: number;
  item_count: number;
  employee_name: string;
  created_at: string;
}

export interface ExpenseItem {
  id: number;
  expense_request_id: number;
  expense_date: string;
  category: string;
  category_display?: string;
  description: string;
  amount: number;
  has_receipt: boolean;
  /** image/jpeg | image/png | application/pdf。未添付時は null/undefined */
  receipt_mime?: string | null;
  display_order: number;
}

export interface ExpenseItemForm {
  expense_date: string;
  category: string;
  description: string;
  amount: number;
}

export interface ExpenseDetail {
  expense: {
    id: number;
    employee_id: number;
    status: string;
    total_amount: number;
    approved_by_id: number | null;
    approved_at: string | null;
    created_at: string;
    updated_at: string;
  };
  items: ExpenseItem[];
  employee_name: string;
  status_display: string;
  status_badge: string;
}

// ── 給与 ──

export interface PayrollRow {
  id: number;
  employee_id: number;
  year_month: string;
  status: string;
  gross_pay: number;
  deduction_total: number;
  net_pay: number;
  employee_code: string;
  last_name: string;
  first_name: string;
}

export interface PayrollSummary {
  total_count: number;
  total_gross: number;
  total_deduction: number;
  total_net: number;
}

export interface PayrollApiResponse {
  payrolls: PayrollRow[];
  summary: PayrollSummary;
}

// ── ユーザー管理 ──

export interface UserRow {
  id: number;
  email: string;
  username: string;
  is_active: boolean;
  is_staff: boolean;
  mfa_enabled: boolean;
  /** 全社員の給与データを閲覧・確認・振込済み操作できるか（is_staffとは独立した権限） */
  can_view_all_payroll: boolean;
  /** 全社員の経費申請を閲覧・承認・差戻しできるか（is_staffとは独立した権限） */
  can_view_all_expenses: boolean;
  created_at: string;
  partner_id: string | null;
  employee_id: number | null;
  employee_name: string | null;
  partner_name: string | null;
  role_display: string | null;
}

// ── 案件（稼働報告提出期限） ──
//
// フィールド名はRust側（src/domain/models/project.rs）のserde出力（snake_case）に
// そのまま合わせている。このコードベースの型定義は既存のSettlementRow等と同様、
// APIレスポンスのキーをcamelCaseに変換せずそのまま使う方針のため。

export type ReportDeadlineType = "RELATIVE" | "FIXED_DAY";

export type ReportDeadlineHolidayRule =
  | "PREVIOUS_BUSINESS_DAY"
  | "NEXT_BUSINESS_DAY";

/** 案件単位の稼働報告提出期限設定（m_project.report_deadline_*） */
export interface ProjectReportDeadline {
  report_deadline_type: ReportDeadlineType;
  /** RELATIVE: 月末からのN営業日前 / FIXED_DAY: 当月N日。未設定はnull（契約側にフォールバック） */
  report_deadline_value: number | null;
  /** FIXED_DAYが非営業日の場合の調整ルール。RELATIVEの場合はnull */
  report_deadline_holiday_rule: ReportDeadlineHolidayRule | null;
}

// ── 取引先メールスレッド要約カード ──

/** メール添付ファイルの種別 */
export type AttachmentKind = "forecast" | "final" | "timesheet" | "other";

/** メール添付情報（カード表示用） */
export interface MailAttachmentInfo {
  email_id: number;
  filename: string;
  kind: AttachmentKind;
  hours_label: string | null;
  timesheet_status: string | null;
}

/** 相手別メールスレッド要約（ダッシュボード表示用） */
export interface MailThreadBrief {
  from_email: string;
  from_name: string;
  company_name: string;
  thread_subject: string;
  summary: string;
  summary_source: "structured" | "ollama";
  last_received_at: string;
  email_count: number;
  needs_choice: boolean;
  attachments: MailAttachmentInfo[];
}

/** GET /api/v1/mail-briefs レスポンス */
export interface MailBriefsResponse {
  briefs: MailThreadBrief[];
}
