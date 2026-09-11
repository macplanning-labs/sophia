// lib/api.ts — API通信ラッパー

import type { SettlementApiResponse, FiltersApiResponse, ApiResult, NotificationsResponse } from "./types";
import { SessionExpiredError, notifySessionExpired } from "./session-expired";

const BASE = "";  // 相対パス。nginx / Next rewrite が /api/* → Rust へ転送

// ── 共通フェッチ ──

/** ログイン系・ログアウト系・招待/マジックリンク確認系・パートナートークン系など、セッション切れモーダルの対象外にしたい画面
 *  （§14: 認証不要な公開ページ。middleware/auth.rsの認証除外パスと揃える） */
function isOnLoginLikePage(): boolean {
  if (typeof window === "undefined") return true;
  const path = window.location.pathname;
  return (
    path.startsWith("/login")
    || path.startsWith("/mfa")
    || path.startsWith("/portal/login")
    || path.startsWith("/portal/auth/")
    || path.startsWith("/token/")
    || path.startsWith("/invite/")
  );
}

/**
 * 401レスポンスを検知したらセッション切れモーダルへ通知し、SessionExpiredError を投げる。
 * 401でなければ何もしない（呼び出し元は通常のエラー処理を続ける）。
 */
async function handleUnauthorized(res: Response): Promise<void> {
  if (res.status !== 401) return;

  let loginUrl = "/login";
  try {
    const body = await res.clone().json();
    if (typeof body?.login_url === "string") loginUrl = body.login_url;
  } catch {
    /* ignore */
  }

  if (!isOnLoginLikePage()) {
    notifySessionExpired(loginUrl);
  }
  throw new SessionExpiredError();
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    credentials: "include", // Cookie転送（セッション共有）
    headers: {
      "Content-Type": "application/json",
      ...init?.headers,
    },
  });

  // 401: セッション切れ → モーダル表示（強制リダイレクトはしない）。
  // 403: 権限不足（MFA未登録含む）はログアウト扱いにしない
  await handleUnauthorized(res);
  if (res.status === 403) {
    let body: { error?: string; mfa_setup_required?: boolean } = {};
    try {
      body = await res.clone().json();
    } catch {
      /* ignore */
    }
    if (
      body.mfa_setup_required &&
      typeof window !== "undefined" &&
      !window.location.pathname.startsWith("/settings/security")
    ) {
      window.location.href = "/settings/security";
    }
    throw new Error(body.error || "権限がありません");
  }

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    let message = `API Error: ${res.status} ${res.statusText}`;
    try { const j = JSON.parse(body); if (j.error) message = j.error; } catch { /* ignore */ }
    throw new Error(message);
  }
  return res.json();
}

// ── CRUD汎用関数 ──

export async function apiPost<T = ApiResult>(url: string, data: unknown): Promise<T> {
  return fetchJson<T>(url, {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export async function apiPut<T = ApiResult>(url: string, data: unknown): Promise<T> {
  return fetchJson<T>(url, {
    method: "PUT",
    body: JSON.stringify(data),
  });
}

export async function apiDelete<T = ApiResult>(url: string): Promise<T> {
  return fetchJson<T>(url, { method: "DELETE" });
}

export async function apiUpload<T = ApiResult>(url: string, formData: FormData): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    credentials: "include",
    body: formData, // Content-Type は自動設定（multipart/form-data）
  });
  await handleUnauthorized(res);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    let message = `Upload Error: ${res.status}`;
    try { const j = JSON.parse(body); if (j.error) message = j.error; } catch { /* ignore */ }
    throw new Error(message);
  }
  return res.json();
}

/** PDF等のバイナリをBlobで取得 */
export async function apiDownload(url: string): Promise<Blob> {
  const res = await fetch(url, { credentials: "include" });
  await handleUnauthorized(res);
  if (!res.ok) throw new Error(`Download Error: ${res.status}`);
  return res.blob();
}

/** Blobをブラウザダウンロードとしてトリガーするヘルパー */
export function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  URL.revokeObjectURL(url);
  a.remove();
}

// ── 月次確定API ──

export async function fetchSettlement(params: {
  month?: string;
  client_id?: string;
  partner_id?: string;
  project_id?: string;
  status?: string;
}): Promise<SettlementApiResponse> {
  const sp = new URLSearchParams();
  if (params.month) sp.set("month", params.month);
  if (params.client_id) sp.set("client_id", params.client_id);
  if (params.partner_id) sp.set("partner_id", params.partner_id);
  if (params.project_id) sp.set("project_id", params.project_id);
  if (params.status) sp.set("status", params.status);
  return fetchJson<SettlementApiResponse>(`${BASE}/api/v1/settlement?${sp}`);
}

export async function fetchFilters(): Promise<FiltersApiResponse> {
  return fetchJson<FiltersApiResponse>(`${BASE}/api/v1/settlement/filters`);
}

export async function issueInvoices(
  selectedIds: number[],
  targetMonth: string
): Promise<ApiResult> {
  return fetchJson<ApiResult>(`${BASE}/api/v1/settlement/issue-invoices`, {
    method: "POST",
    body: JSON.stringify({ selected_ids: selectedIds, target_month: targetMonth }),
  });
}

export async function issueNotices(
  selectedIds: number[],
  targetMonth: string
): Promise<ApiResult> {
  return fetchJson<ApiResult>(`${BASE}/api/v1/settlement/issue-notices`, {
    method: "POST",
    body: JSON.stringify({ selected_ids: selectedIds, target_month: targetMonth }),
  });
}

// ── ダッシュボードAPI ──

import type { DashboardApiResponse, ContractRow, OrderRow, NoticeRow, ProjectSummary } from "./types";

export async function fetchDashboard(): Promise<DashboardApiResponse> {
  return fetchJson<DashboardApiResponse>(`${BASE}/api/v1/dashboard`);
}

export async function fetchProjectDashboard(month: string): Promise<ProjectSummary[]> {
  return fetchJson<ProjectSummary[]>(`${BASE}/api/v1/dashboard/projects?month=${month}`);
}

export async function fetchNotifications(): Promise<NotificationsResponse> {
  return fetchJson<NotificationsResponse>(`${BASE}/api/v1/notifications`);
}

/** 取引先別メールスレッド要約を取得 */
export async function fetchMailBriefs(): Promise<MailBriefsResponse> {
  return fetchJson<MailBriefsResponse>(`${BASE}/api/v1/mail-briefs`);
}

// ── 発注契約API ──

export async function fetchPartnerContracts(): Promise<ContractRow[]> {
  return fetchJson<ContractRow[]>(`${BASE}/api/v1/partner-contracts`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPartnerContractFormData(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/partner-contracts/form-data`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchOrderFormData(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/orders/form-data`);
}

// ── 発注書API ──

export async function fetchOrders(params?: {
  partner?: string;
  status?: string;
}): Promise<OrderRow[]> {
  const sp = new URLSearchParams();
  if (params?.partner) sp.set("partner", params.partner);
  if (params?.status) sp.set("status", params.status);
  return fetchJson<OrderRow[]>(`${BASE}/api/v1/orders?${sp}`);
}

// ── 支払通知API ──

export async function fetchNotices(params?: {
  partner?: string;
}): Promise<NoticeRow[]> {
  const sp = new URLSearchParams();
  if (params?.partner) sp.set("partner", params.partner);
  return fetchJson<NoticeRow[]>(`${BASE}/api/v1/notices?${sp}`);
}

/** 発注書削除 */
export async function deleteOrder(id: string): Promise<ApiResult> {
  return apiDelete<ApiResult>(`${BASE}/api/v1/orders/${id}`);
}

/** 受注書削除 */
export async function deleteReceivedOrder(id: string | number): Promise<ApiResult> {
  return apiDelete<ApiResult>(`${BASE}/api/v1/received-orders/${id}`);
}

/** 請求書削除 */
export async function deleteInvoice(id: string | number): Promise<ApiResult> {
  return apiPost<ApiResult>(`${BASE}/api/v1/invoices/${id}/delete`, {});
}

/** 支払通知削除 */
export async function deleteNotice(id: string): Promise<ApiResult> {
  return apiPost<ApiResult>(`${BASE}/api/v1/notices/${id}/delete`, {});
}

// ── Phase 3 APIs ──

import type {
  ClientContractRow, ReceivedOrderRow, ReceivedEmailRow, InvoiceRow,
  TimesheetApiResponse, TaskApiResponse,
  EmployeeRow, ExpenseRow, ExpenseItemForm, ExpenseDetail, PayrollApiResponse, UserRow, CompanyInfo,
  ProjectRow, WizardResult, MailBriefsResponse,
} from "./types";

export async function fetchClientContracts(): Promise<ClientContractRow[]> {
  return fetchJson<ClientContractRow[]>(`${BASE}/api/v1/client-contracts`);
}

// ── 案件(/projects) ──

export async function fetchProjects(): Promise<ProjectRow[]> {
  return fetchJson<ProjectRow[]>(`${BASE}/api/v1/projects`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchProjectDetail(projectId: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/projects/${projectId}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchProjectWizardFormData(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/projects/wizard/form-data`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function submitProjectWizard(body: any): Promise<WizardResult> {
  return apiPost<WizardResult>(`${BASE}/api/v1/projects/wizard`, body);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function updateProject(projectId: string, body: any): Promise<ApiResult> {
  return apiPut<ApiResult>(`${BASE}/api/v1/projects/${projectId}`, body);
}

export async function deleteProject(projectId: string): Promise<ApiResult> {
  return apiDelete<ApiResult>(`${BASE}/api/v1/projects/${projectId}`);
}

export async function fetchReceivedOrders(): Promise<ReceivedOrderRow[]> {
  return fetchJson<ReceivedOrderRow[]>(`${BASE}/api/v1/received-orders`);
}

/** 受注契約から受注書を作成 */
export async function createReceivedOrderFromContract(payload: {
  client_contract_id: number;
  target_month: string; // YYYY-MM-01
  work_start: string;   // YYYY-MM-DD
  work_end: string;     // YYYY-MM-DD
}): Promise<{ id: number; received_order_no: string }> {
  return apiPost(`${BASE}/api/v1/received-orders`, payload);
}

/** 発注契約IDのリストから発注書を一括作成（パートナー別グルーピングはバックエンド側で実施） */
export async function createOrdersFromContracts(payload: {
  contract_ids: number[];
  work_start: string; // YYYY-MM-DD
  work_end: string;   // YYYY-MM-DD
}): Promise<{ success: boolean; order_ids?: string[]; skipped_partners?: string[]; error?: string }> {
  return apiPost(`${BASE}/api/v1/orders`, payload);
}

export async function fetchReceivedEmails(needsReview: boolean, knownDomainOnly: boolean): Promise<{ emails: ReceivedEmailRow[] }> {
  return fetchJson(`${BASE}/api/v1/received-emails?needs_review=${needsReview}&known_domain_only=${knownDomainOnly}`);
}

export async function fetchInvoices(): Promise<InvoiceRow[]> {
  return fetchJson<InvoiceRow[]>(`${BASE}/api/v1/invoices`);
}

export async function fetchTimesheets(): Promise<TimesheetApiResponse> {
  return fetchJson<TimesheetApiResponse>(`${BASE}/api/v1/timesheets`);
}

export async function fetchTasks(params?: {
  month?: string; status?: string;
}): Promise<TaskApiResponse> {
  const sp = new URLSearchParams();
  if (params?.month) sp.set("month", params.month);
  if (params?.status) sp.set("status", params.status);
  return fetchJson<TaskApiResponse>(`${BASE}/api/v1/tasks?${sp}`);
}

export async function fetchEmployees(): Promise<EmployeeRow[]> {
  return fetchJson<EmployeeRow[]>(`${BASE}/api/v1/employees`);
}

export async function fetchExpenses(): Promise<ExpenseRow[]> {
  return fetchJson<ExpenseRow[]>(`${BASE}/api/v1/expenses`);
}

export async function createExpense(data: {
  employee_id: number;
  items: ExpenseItemForm[];
}): Promise<{ success: boolean; id?: number; item_ids?: number[]; error?: string }> {
  return apiPost(`${BASE}/api/v1/expenses`, data);
}

export async function addExpenseItem(expenseId: number, item: ExpenseItemForm): Promise<{ success: boolean; id?: number; error?: string }> {
  return apiPost(`${BASE}/api/v1/expenses/${expenseId}/items`, item);
}

export async function updateExpenseItem(itemId: number, item: ExpenseItemForm): Promise<{ success: boolean; error?: string }> {
  return apiPut(`${BASE}/api/v1/expenses/items/${itemId}`, item);
}

export async function deleteExpenseItem(itemId: number): Promise<{ success: boolean; error?: string }> {
  return apiDelete(`${BASE}/api/v1/expenses/items/${itemId}`);
}

export function expenseItemReceiptUrl(itemId: number): string {
  return `${BASE}/api/v1/expenses/items/${itemId}/receipt`;
}

/** PC画面からの領収書アップロード（ファイル選択・クリップボード貼り付け） */
export async function uploadExpenseItemReceipt(
  itemId: number,
  file: File,
): Promise<{ success: boolean; error?: string }> {
  const form = new FormData();
  form.append("file", file);
  return apiUpload(`${BASE}/api/v1/expenses/items/${itemId}/receipt`, form);
}

export async function issueMobileUploadToken(itemId: number): Promise<{ success: boolean; token?: string; upload_url?: string; expires_at?: string; error?: string }> {
  return apiPost(`${BASE}/api/v1/expenses/items/${itemId}/mobile-upload/token`, {});
}

export async function fetchMobileUploadStatus(token: string): Promise<{
  uploaded: boolean;
  expired: boolean;
  expense_request_item_id?: number;
  receipt_mime?: string | null;
  error?: string;
}> {
  return fetchJson(`${BASE}/api/v1/mobile-upload/${token}/status`);
}

export async function uploadMobileReceipt(token: string, file: File): Promise<{ success: boolean; error?: string }> {
  const form = new FormData();
  form.append("file", file);
  const res = await fetch(`${BASE}/api/v1/mobile-upload/${token}`, {
    method: "POST",
    body: form,
  });
  return res.json();
}

export async function fetchEmployeeOptions(): Promise<{ id: number; display_name: string }[]> {
  return fetchJson(`${BASE}/api/v1/employees/options`);
}

export async function fetchEngineerOptions(): Promise<{ id: number; name: string; email: string | null }[]> {
  return fetchJson(`${BASE}/api/v1/engineers/options`);
}

// ── ログインユーザー情報 ──

export interface CurrentUser {
  user_id: number;
  role: "ADMIN" | "EMPLOYEE" | "PARTNER" | "ENGINEER" | "ANONYMOUS";
  email: string;
  can_view_all_expenses?: boolean;
  can_view_all_payroll?: boolean;
  mfa_enabled?: boolean;
  mfa_setup_required?: boolean;
}

export async function fetchCurrentUser(): Promise<CurrentUser> {
  return fetchJson(`${BASE}/api/v1/auth/me`);
}

export async function fetchExpenseCategoryOptions(): Promise<{ code: string; name: string }[]> {
  return fetchJson(`${BASE}/api/v1/expenses/categories`);
}

export async function updateExpense(id: number, data: {
  employee_id: number;
}): Promise<{ success: boolean; error?: string }> {
  return apiPut(`${BASE}/api/v1/expenses/${id}`, data);
}

export async function deleteExpense(id: number): Promise<{ success: boolean; error?: string }> {
  return apiDelete(`${BASE}/api/v1/expenses/${id}`);
}

export async function fetchPayroll(month?: string): Promise<PayrollApiResponse> {
  const sp = new URLSearchParams();
  if (month) sp.set("month", month);
  return fetchJson<PayrollApiResponse>(`${BASE}/api/v1/payroll?${sp}`);
}

export async function fetchUsers(): Promise<UserRow[]> {
  return fetchJson<UserRow[]>(`${BASE}/api/v1/users`);
}

// ── 詳細API ──

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPartnerContractDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/partner-contracts/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchOrderDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/orders/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchNoticeDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/notices/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchClientContractDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/client-contracts/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchReceivedOrderDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/received-orders/${id}`);
}

/** 受注書に受注契約を手動紐付け */
export async function linkReceivedOrderContract(
  id: string | number,
  clientContractId: number
): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/received-orders/${id}/link-contract`, {
    client_contract_id: clientContractId,
  });
}

// ── 稼働報告 自己申告（案件に紐づかない社員向け）──

export interface SelfReportDailyEntry {
  day: number;
  start: string;
  end: string;
  break_minutes: number;
  off: boolean;
}

export interface SelfReportSheet {
  id: number;
  status: string;
  target_month: string;
  daily_data: SelfReportDailyEntry[] | null;
  total_hours: string;
  work_days: number;
  overtime_hours: string;
  night_hours: string;
  holiday_hours: string;
  sheet_file_id: string;
}

export async function fetchSelfReport(month: string): Promise<{ sheet: SelfReportSheet | null }> {
  return fetchJson(`${BASE}/api/v1/timesheets/self-report?month=${month}`);
}

export async function submitSelfReport(payload: {
  target_month: string;
  daily_data: SelfReportDailyEntry[];
  night_hours: number;
}): Promise<ApiResult & { message?: string }> {
  return apiPost(`${BASE}/api/v1/timesheets/self-report`, payload);
}

export async function prepareSelfReportSheet(target_month: string): Promise<ApiResult & { url?: string }> {
  return apiPost(`${BASE}/api/v1/timesheets/self-report/sheets/prepare`, { target_month });
}

export async function submitSelfReportSheet(target_month: string): Promise<ApiResult & { message?: string }> {
  return apiPost(`${BASE}/api/v1/timesheets/self-report/sheets/submit`, { target_month });
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchInvoiceDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/invoices/${id}`);
}

/// 送信メール本文プレビュー（承認後、送信モーダルの初期値として使う）
export async function fetchInvoiceEmailPreview(id: string): Promise<{
  subject: string;
  body: string;
  to_email?: string | null;
  cc_email?: string | null;
  client_name?: string;
}> {
  return fetchJson(`${BASE}/api/v1/invoices/${id}/email-preview`);
}

/// 発注書送信メール本文プレビューを取得
export async function fetchOrderEmailPreview(id: string): Promise<{ subject: string; body: string }> {
  return fetchJson(`${BASE}/api/v1/orders/${id}/email-preview`);
}

/// 支払通知書送信メール本文プレビューを取得
export async function fetchNoticeEmailPreview(id: string): Promise<{
  subject: string;
  body: string;
  to_email?: string | null;
  cc_email?: string | null;
  partner_name?: string;
}> {
  return fetchJson(`${BASE}/api/v1/notices/${id}/email-preview`);
}

export async function sendNoticeMail(
  id: string,
  payload: { subject: string; body: string }
): Promise<ApiResult & { message?: string; token_url?: string }> {
  return apiPost(`${BASE}/api/v1/notices/${id}/send`, payload);
}

/// 稼働報告提出依頼メール本文プレビューを取得
export async function fetchOrderTimesheetRequestPreview(id: string): Promise<{
  subject: string;
  body: string;
  to_email?: string | null;
  partner_name?: string;
}> {
  return fetchJson(`${BASE}/api/v1/orders/${id}/request-timesheet-preview`);
}

export async function approveInvoice(id: string): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/invoices/${id}/approve`, {});
}

export async function rejectInvoice(id: string): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/invoices/${id}/reject`, {});
}

export async function sendInvoiceMail(
  id: string,
  payload: { subject: string; body: string }
): Promise<ApiResult & { message?: string; to_email?: string; token_url?: string }> {
  return apiPost(`${BASE}/api/v1/invoices/${id}/send`, payload);
}

export async function sendInvoicePeppol(id: string): Promise<ApiResult & { message_id?: string; status?: string }> {
  return apiPost(`${BASE}/api/v1/invoices/${id}/peppol/send`, {});
}

export async function sendNoticePeppol(id: string): Promise<ApiResult & { message_id?: string; status?: string }> {
  return apiPost(`${BASE}/api/v1/notices/${id}/peppol/send`, {});
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPeppolTransmissions(status?: string): Promise<any[]> {
  const qs = status ? `?status=${encodeURIComponent(status)}` : "";
  return fetchJson(`${BASE}/api/v1/peppol/transmissions${qs}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchTimesheetDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/timesheets/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPayrollDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/payroll/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchEmployeeDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/employees/${id}`);
}

export async function fetchExpenseDetail(id: string): Promise<ExpenseDetail> {
  return fetchJson(`${BASE}/api/v1/expenses/${id}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchUserDetail(id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/users/${id}`);
}

// ── セキュリティAPI ──

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchSecurity(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/security`);
}

// ── 自社情報設定API ──

export async function fetchCompanyInfo(): Promise<{ success: boolean; company_info: CompanyInfo; error?: string }> {
  return fetchJson(`${BASE}/api/v1/company-info`);
}

export async function updateCompanyInfo(data: Record<string, unknown>): Promise<ApiResult> {
  return apiPut(`${BASE}/api/v1/company-info`, data);
}

/** 保存済みのSMTP設定でログイン中の管理者自身にテストメールを送信する */
export async function testCompanyEmail(): Promise<ApiResult & { to?: string }> {
  return apiPost(`${BASE}/api/v1/company-info/test-email`, {});
}

// ── マスタメンテAPI ──

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchMastersMeta(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/masters/meta`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchMastersData(table: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/masters/${table}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchMasterDetail(table: string, id: string): Promise<any> {
  return fetchJson(`${BASE}/api/v1/masters/${table}/${id}`);
}

/// fk_selectカラムごとの選択肢一覧（{ カラム名: [{value,label}, ...] }）
export async function fetchMasterFkOptions(table: string): Promise<Record<string, { value: string; label: string }[]>> {
  return fetchJson(`${BASE}/api/v1/masters/${table}/fk-options`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function createMasterRecord(table: string, data: Record<string, unknown>): Promise<any> {
  return apiPost(`${BASE}/api/v1/masters/${table}`, data);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function updateMasterRecord(table: string, id: string, data: Record<string, unknown>): Promise<any> {
  return apiPut(`${BASE}/api/v1/masters/${table}/${id}`, data);
}

export async function deleteMasterRecord(table: string, id: string): Promise<void> {
  await apiDelete(`${BASE}/api/v1/masters/${table}/${id}`);
}

// ── EDI操作API ──

export async function fetchEdiOrders(year: number, month: number): Promise<{ orders: EdiOrder[]; imported_order_numbers: string[] }> {
  return fetchJson(`${BASE}/api/v1/edi/orders?year=${year}&month=${month}`);
}

export async function importEdiOrder(orderId: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/edi/orders/${orderId}/import`, {});
}

export async function fetchEdiInvoices(year: number, month: number): Promise<{ invoices: EdiInvoice[] }> {
  return fetchJson(`${BASE}/api/v1/edi/invoices?year=${year}&month=${month}`);
}

export async function approveEdiInvoice(invoiceId: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/edi/invoices/${invoiceId}/approve`, {});
}

export async function triggerEdiImport(): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/edi/import`, {});
}

export async function triggerBillingImport(year: number, month: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/settlement/import-billings`, { year, month });
}

export async function triggerImportAll(year: number, month: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/edi/import-all`, { year, month });
}

// ── メール操作API ──

export async function fetchMailLogs(): Promise<MailLog[]> {
  return fetchJson(`${BASE}/api/v1/dashboard`).then((d: any) => d.mail_logs ?? []);
}

export async function triggerMailFetch(): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/mail/fetch`, {});
}

export async function confirmMail(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/mail/${id}/confirm`, {});
}

export async function unconfirmMail(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/mail/${id}/unconfirm`, {});
}

export async function fetchImapLockStatus(): Promise<{ locked: boolean; reason: string | null }> {
  return fetchJson(`${BASE}/api/v1/mail/imap-lock-status`);
}

/** IMAP接続確認（実際のメール取得は行わない） */
export async function testImapConnection(): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/mail/imap-test`, {});
}

export async function unlockImap(): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/mail/imap-unlock`, {});
}

// ── 給与操作API ──

export async function calculatePayroll(month: string): Promise<ApiResult & {
  calculated?: number;
  skipped?: number;
  errors?: string[];
}> {
  return apiPost(`${BASE}/api/v1/payroll/calculate`, { month });
}

export async function confirmPayroll(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/payroll/${id}/confirm`, {});
}

export async function recalculatePayrollDeductions(id: string | number): Promise<ApiResult & {
  deductions?: Record<string, number>;
  message?: string;
}> {
  return apiPost(`${BASE}/api/v1/payroll/${id}/recalculate-deductions`, {});
}

export async function markPayrollPaid(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/payroll/${id}/paid`, {});
}

// ── 経費承認API ──

export async function approveExpense(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/expenses/${id}/approve`, {});
}

export async function rejectExpense(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/expenses/${id}/reject`, {});
}

/** 承認取り消し（APPROVED → PENDING。精算済は不可） */
export async function unapproveExpense(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/expenses/${id}/unapprove`, {});
}

/** 再申請（REJECTED → PENDING） */
export async function resubmitExpense(id: number): Promise<ApiResult> {
  return apiPost(`${BASE}/api/v1/expenses/${id}/resubmit`, {});
}

// ── ポータルAPI ──

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalOrders(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/portal/orders`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function approvePortalOrder(orderId: string): Promise<any> {
  return apiPost(`${BASE}/api/v1/portal/orders/${orderId}/approve`, {});
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalNotices(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/portal/notices`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function confirmPortalNotice(noticeId: string): Promise<any> {
  return apiPost(`${BASE}/api/v1/portal/notices/${noticeId}/confirm`, {});
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalTimesheets(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/portal/timesheets`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function submitPortalTimesheet(id: string): Promise<any> {
  return apiPost(`${BASE}/api/v1/portal/timesheets/${id}/submit`, {});
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalEngineers(): Promise<any> {
  return fetchJson(`${BASE}/api/v1/portal/engineers`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalWorkEntries(params: { engineer_id: string; year: number; month: number }): Promise<any> {
  const sp = new URLSearchParams();
  sp.set("engineer_id", params.engineer_id);
  sp.set("year", params.year.toString());
  sp.set("month", params.month.toString());
  return fetchJson(`${BASE}/api/v1/portal/work-entries?${sp}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function fetchPortalWorkEntriesSummary(params: { engineer_id: string; year: number; month: number }): Promise<any> {
  const sp = new URLSearchParams();
  sp.set("engineer_id", params.engineer_id);
  sp.set("year", params.year.toString());
  sp.set("month", params.month.toString());
  return fetchJson(`${BASE}/api/v1/portal/work-entries/summary?${sp}`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function submitPortalWorkEntries(data: unknown): Promise<any> {
  return apiPost(`${BASE}/api/v1/portal/work-entries`, data);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function invitePortalEngineer(data: unknown): Promise<any> {
  return apiPost(`${BASE}/api/v1/portal/invite`, data);
}

// ── 型定義 ──

export type EdiOrder = {
  id: number;
  order_no: string;
  project_name: string;
  worker_name: string;
  amount: number;
  status: string;
  is_imported: boolean;
};

export type EdiInvoice = {
  id: number;
  invoice_no: string;
  project_name: string;
  amount: number;
  status: string;
  is_approved: boolean;
};

export type MailLog = {
  id: number;
  sender_name: string;
  sender_email: string;
  subject: string;
  received_at: string;
  classification: string;
  is_reflected: boolean;
};

// ── APIキー管理（/settings/api-keys、Admin専用）──

export interface ApiKeyItem {
  id: string;
  key_prefix: string;
  scope: string;
  name: string;
  is_active: boolean;
  created_at: string;
  revoked_at?: string;
}

export interface GeneratedApiKey {
  id: string;
  api_key: string;
  key_prefix: string;
}

export async function fetchApiKeys(clientId: number): Promise<ApiKeyItem[]> {
  return fetchJson<ApiKeyItem[]>(`${BASE}/api/v1/settings/api-keys?client_id=${clientId}`);
}

export async function generateApiKey(params: {
  client_id: number;
  name: string;
  scope: string;
  environment: string;
}): Promise<GeneratedApiKey> {
  return apiPost<GeneratedApiKey>(`${BASE}/api/v1/settings/api-keys`, params);
}

export async function revokeApiKey(id: string): Promise<ApiResult> {
  return apiPost<ApiResult>(`${BASE}/api/v1/settings/api-keys/${id}/revoke`, {});
}

