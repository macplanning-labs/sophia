/**
 * 一覧画面の「要対応」判定 — WS6
 * 旧 PipelineSection の filterStatus="pending"(完了除外) と同趣旨。
 */

export type ListFilterMode = "pending" | "all";

const ORDER_DONE = new Set(["PAID", "CANCELLED", "REJECTED"]);
const RECEIVED_ORDER_DONE = new Set(["PAID"]);
const TIMESHEET_DONE = new Set(["APPROVED", "SENT"]);
const INVOICE_DONE = new Set(["SENT", "PAID", "ISSUED"]);

export function isOrderNeedsAction(status: string): boolean {
  return !ORDER_DONE.has(status);
}

export function isReceivedOrderNeedsAction(status: string): boolean {
  return !RECEIVED_ORDER_DONE.has(status);
}

export function isTimesheetNeedsAction(status: string): boolean {
  return !TIMESHEET_DONE.has(status);
}

export function isNoticeNeedsAction(confirmed: boolean): boolean {
  return !confirmed;
}

export function isInvoiceNeedsAction(status: string): boolean {
  return !INVOICE_DONE.has(status);
}

export function applyNeedsActionFilter<T>(
  rows: T[],
  mode: ListFilterMode,
  predicate: (row: T) => boolean,
): T[] {
  if (mode === "all") return rows;
  return rows.filter(predicate);
}
