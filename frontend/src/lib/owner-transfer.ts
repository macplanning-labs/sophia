import type { QueryClient } from "@tanstack/react-query";
// lib/owner-transfer.ts — 担当(組織・担当者)の付け替え・退職時の引き継ぎ・自動取込の受け皿の API と補助関数。
//
// バックエンド: src/presentation/handlers/org_admin/{transfer,owner_view,receiver}.rs（DEMO-000173 PR 6b/6c）。Admin 専用。
// 社員の削除・ユーザーの無効化は、担当データがあると 409 `{error, owned:[{table,count}], total}` を返す。
// 引き継ぎ先(transfer_to)を付けて同じ要求を送り直すと、引き継いで無効化する。

import { apiGet, apiPost, apiPut, requestJsonWithStatus } from "@/lib/api";

// ── 型 ──

/** 担当データの種類ごとの件数(サーバの TableCount)。 */
export interface OwnedCount {
  table: string;
  count: number;
}

/** 社員が担当のデータを、組織ごとに数えたもの。org_unit_id / org_name は未設定だと null。 */
export interface OwnedByOrg {
  org_unit_id: number | null;
  org_name: string | null;
  count: number;
}

export interface OwnedByEmployee {
  owned: OwnedCount[];
  by_org: OwnedByOrg[];
  total: number;
}

/** 顧客・案件の今の担当。未設定の項目は null。 */
export interface OwnerView {
  org_unit_id: number | null;
  org_name: string | null;
  employee_id: number | null;
  employee_name: string | null;
}

/** 引き継ぎ先(組織と担当者)。両方必須。 */
export interface TransferTo {
  org_unit_id: number;
  employee_id: number;
}

/** 担当を変える対象。顧客(数値ID)か案件(案件コード)のどちらか一方。 */
export type OwnerTarget = { client_id: number } | { project_id: string };

export interface ReceiverView {
  org_unit_id: number | null;
  employee_id: number | null;
}

export interface OwnerTransferResult {
  updated: OwnedCount[];
  total: number;
}

/** 画面に出すテーブル名(担当データの種類)。サーバが返す 11 種類すべてを網羅する。 */
export const TABLE_LABELS: Record<string, string> = {
  m_client: "顧客",
  m_project: "案件",
  m_client_contract: "契約(売上)",
  m_partner_contract: "契約(仕入)",
  t_purchase_order: "発注",
  t_payment_notice: "支払通知",
  t_received_order: "受注",
  t_billing_invoice: "請求書",
  t_monthly_timesheet: "稼働報告",
  t_monthly_task: "月次タスク",
  s_company_info: "自動取込の受け皿",
};

/** 社員一覧のキャッシュキー。担当の選択肢と受け皿の表示で共有する。 */
export const EMPLOYEE_LIST_QUERY_KEY = ["owner-transfer", "employees"] as const;

// ── API ──

function ownerQuery(target: OwnerTarget): string {
  return "client_id" in target
    ? `client_id=${encodeURIComponent(String(target.client_id))}`
    : `project_id=${encodeURIComponent(target.project_id)}`;
}

export async function fetchOwnerCurrent(target: OwnerTarget): Promise<OwnerView> {
  return apiGet<OwnerView>(`/api/v1/admin/owner-transfer/current?${ownerQuery(target)}`);
}

export async function transferOwner(target: OwnerTarget, to: TransferTo): Promise<OwnerTransferResult> {
  return apiPost<OwnerTransferResult>("/api/v1/admin/owner-transfer", { ...target, to });
}

export async function fetchOwnedByEmployee(employeeId: number): Promise<OwnedByEmployee> {
  return apiGet<OwnedByEmployee>(`/api/v1/admin/owner-transfer/by-employee/${employeeId}`);
}

export async function fetchAutoImportReceiver(): Promise<ReceiverView> {
  return apiGet<ReceiverView>("/api/v1/admin/auto-import-receiver");
}

export async function updateAutoImportReceiver(to: TransferTo): Promise<ReceiverView> {
  return apiPut<ReceiverView>("/api/v1/admin/auto-import-receiver", to);
}

/** 409 の本文から、担当データの件数を取り出す。409 以外や形が違うときは null。 */
export function parseOwnedConflict(status: number, body: unknown): OwnedCount[] | null {
  if (status !== 409 || typeof body !== "object" || body === null) return null;
  const owned = (body as { owned?: unknown }).owned;
  if (!Array.isArray(owned)) return null;
  return owned.filter(
    (c): c is OwnedCount =>
      typeof c === "object" && c !== null && typeof (c as OwnedCount).table === "string" && typeof (c as OwnedCount).count === "number",
  );
}

/** 失敗時のメッセージ。サーバの {error} を優先する。 */
function errorText(body: unknown, status: number): string {
  if (typeof body === "object" && body !== null && typeof (body as { error?: unknown }).error === "string") {
    return (body as { error: string }).error;
  }
  return `API Error: ${status}`;
}

/** 担当データがあって引き継ぎ先を求められた(409)か、通常の成功か。 */
export type ConflictableResult = { conflict: false; body: unknown } | { conflict: true; owned: OwnedCount[] };

/** 社員を削除(退職扱い)する。担当データがあり transferTo が無いと、conflict を返す。 */
export async function deleteEmployeeWithTransfer(employeeId: number, transferTo?: TransferTo): Promise<ConflictableResult> {
  const r = await requestJsonWithStatus("DELETE", `/api/v1/employees/${employeeId}`, transferTo ? { transfer_to: transferTo } : undefined);
  return settle(r);
}

/** ユーザーの有効/無効を切り替える。無効にするとき担当データがあり transferTo が無いと、conflict を返す。 */
export async function toggleUserWithTransfer(userId: number, transferTo?: TransferTo): Promise<ConflictableResult> {
  const r = await requestJsonWithStatus("POST", `/api/v1/users/${userId}/toggle`, transferTo ? { transfer_to: transferTo } : {});
  return settle(r);
}

function settle(r: { ok: boolean; status: number; body: unknown }): ConflictableResult {
  const owned = parseOwnedConflict(r.status, r.body);
  if (owned) return { conflict: true, owned };
  if (!r.ok) throw new Error(errorText(r.body, r.status));
  return { conflict: false, body: r.body };
}

// ── 純粋な補助関数 ──

export function tableLabel(table: string): string {
  return TABLE_LABELS[table] ?? table;
}

export interface HandOverSummary {
  total: number;
  /** 件数が 1 以上の担当データの種類(TABLE_LABELS の順) */
  rows: { table: string; label: string; count: number }[];
  /** 組織ごとの内訳(件数が 1 以上)。組織が未設定のものは「組織未設定」 */
  orgLines: { label: string; count: number }[];
}

/**
 * 引き継ぎの確認画面に出す内容を作る。
 * total は 409 の件数(owned)の合計。内訳(byOrg)は社員の詳細取得からのもので、無いときは空。
 */
export function buildHandOverSummary(owned: OwnedCount[], byOrg: OwnedByOrg[] = []): HandOverSummary {
  const order = Object.keys(TABLE_LABELS);
  const rank = (t: string) => {
    const i = order.indexOf(t);
    return i === -1 ? order.length : i;
  };
  const rows = owned
    .filter((c) => c.count > 0)
    .slice()
    .sort((a, b) => rank(a.table) - rank(b.table))
    .map((c) => ({ table: c.table, label: tableLabel(c.table), count: c.count }));
  const orgLines = byOrg
    .filter((o) => o.count > 0)
    .map((o) => ({ label: o.org_name ?? "組織未設定", count: o.count }));
  return {
    total: owned.reduce((sum, c) => sum + c.count, 0),
    rows,
    orgLines,
  };
}

/**
 * 担当を変えたあとに、画面に出ているデータを読み直す。
 * 担当の付け替えは配下(案件・契約・発注・受注・請求など)にも広がり、取引先・案件の一覧にも担当が出るため、
 * 「今の担当」だけでなく、表示中のすべての一覧を読み直す(変更は管理者のたまの操作なので、まとめて読み直してよい)。
 * 以前は「今の担当」だけを読み直していて、取引先の一覧が再読み込みまで「未設定」のままだった。
 */
export function refreshAfterOwnerChange(qc: Pick<QueryClient, "invalidateQueries">): Promise<void> {
  return qc.invalidateQueries();
}
