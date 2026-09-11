import type { SettlementViewRow } from "@/lib/types";

export type ReadinessStatus = "ok" | "missing" | "warning" | "done" | "na";

export interface ReadinessCheck {
  id: string;
  label: string;
  status: ReadinessStatus;
  detail?: string;
  linkPath?: string;
  linkLabel?: string;
}

const PO_STATUS_LABEL: Record<string, string> = {
  DRAFT: "起票",
  SENT: "送付済",
  ACCEPTED: "受諾済",
  REPORT_RECEIVED: "報告書受領",
  NOTICE_CREATED: "支払通知作成",
  NOTICE_CONFIRMED: "支払通知受諾",
  PAID: "支払済",
};

function poStatusLabel(status: string): string {
  return PO_STATUS_LABEL[status] ?? status;
}

function isPoReady(status: string | null | undefined): boolean {
  return status === "ACCEPTED" || status === "REPORT_RECEIVED";
}

/** 請求書一括発行の行別チェックリスト */
export function getInvoiceReadinessChecks(
  vr: SettlementViewRow,
  monthLabel: string
): ReadinessCheck[] {
  const r = vr.row;
  const checks: ReadinessCheck[] = [];

  if (r.invoice_issued) {
    return [
      {
        id: "issued",
        label: "請求書",
        status: "done",
        detail: "発行済み",
        linkPath: "/invoices",
        linkLabel: "請求書一覧",
      },
    ];
  }

  if (r.client_edi_system_type === "EDI_OASIS") {
    checks.push({
      id: "edi",
      label: "請求方法",
      status: "na",
      detail: "EDI連携クライアント（Sophiaからの発行対象外）",
    });
    return checks;
  }

  if (!r.timesheet_id) {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "missing",
      detail: `${monthLabel}分が未提出`,
      linkPath: "/timesheets?upload=1",
      linkLabel: "稼働報告を登録",
    });
  } else if (r.timesheet_status !== "APPROVED") {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "missing",
      detail: `${monthLabel}分が未承認`,
      linkPath: `/timesheets?edit=${r.timesheet_id}`,
      linkLabel: "承認画面へ",
    });
  } else {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "ok",
      detail: "承認済み",
    });
  }

  return checks;
}

/** 支払通知一括発行の行別チェックリスト */
export function getNoticeReadinessChecks(
  vr: SettlementViewRow,
  monthLabel: string
): ReadinessCheck[] {
  const r = vr.row;
  const checks: ReadinessCheck[] = [];

  if (r.notice_issued) {
    return [
      {
        id: "issued",
        label: "支払通知",
        status: "done",
        detail: "発行済み",
        linkPath: "/notices",
        linkLabel: "支払通知一覧",
      },
    ];
  }

  if (r.partner_contract_id == null) {
    checks.push({
      id: "partner_contract",
      label: "発注契約",
      status: "missing",
      detail: "未登録（自社エンジニア扱い）",
      linkPath: "/partner-contracts",
      linkLabel: "発注契約を登録",
    });
    return checks;
  }

  checks.push({
    id: "partner_contract",
    label: "発注契約",
    status: "ok",
    detail: "登録済み",
  });

  if (!r.timesheet_id) {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "missing",
      detail: `${monthLabel}分が未提出`,
      linkPath: "/timesheets?upload=1",
      linkLabel: "稼働報告を登録",
    });
  } else if (r.timesheet_status !== "APPROVED") {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "missing",
      detail: `${monthLabel}分が未承認`,
      linkPath: `/timesheets?edit=${r.timesheet_id}`,
      linkLabel: "承認画面へ",
    });
  } else {
    checks.push({
      id: "timesheet",
      label: "稼働報告",
      status: "ok",
      detail: "承認済み",
    });
  }

  if (!r.purchase_order_id) {
    checks.push({
      id: "purchase_order",
      label: "発注書",
      status: "missing",
      detail: `${monthLabel}分の発注書がありません`,
      linkPath: "/orders",
      linkLabel: "発注一覧を開く",
    });
  } else if (!isPoReady(r.purchase_order_status)) {
    const label = poStatusLabel(r.purchase_order_status ?? "");
    const isWaiting = r.purchase_order_status === "SENT";
    checks.push({
      id: "purchase_order",
      label: "発注書",
      status: isWaiting ? "warning" : "missing",
      detail: `${r.purchase_order_id} は「${label}」（受諾済または報告書受領が必要）`,
      linkPath: `/orders/${encodeURIComponent(r.purchase_order_id)}`,
      linkLabel: "発注書詳細を開く",
    });
  } else {
    checks.push({
      id: "purchase_order",
      label: "発注書",
      status: "ok",
      detail: `${r.purchase_order_id}（${poStatusLabel(r.purchase_order_status ?? "")}）`,
      linkPath: `/orders/${encodeURIComponent(r.purchase_order_id)}`,
      linkLabel: "発注書を確認",
    });
  }

  return checks;
}

export function getReadinessChecks(
  vr: SettlementViewRow,
  mode: "billing" | "payment",
  monthLabel: string
): ReadinessCheck[] {
  return mode === "billing"
    ? getInvoiceReadinessChecks(vr, monthLabel)
    : getNoticeReadinessChecks(vr, monthLabel);
}

export function isReadyForIssue(checks: ReadinessCheck[]): boolean {
  return checks.every((c) => c.status === "ok");
}

export function isExcludedFromIssue(checks: ReadinessCheck[]): boolean {
  return checks.some((c) => c.status === "na" || c.status === "done");
}

export interface ReadinessIssue {
  engineerName: string;
  context: string;
  check: ReadinessCheck;
}

/** 未達のチェック項目を一覧化（パネル表示用） */
export function collectReadinessIssues(
  rows: SettlementViewRow[],
  mode: "billing" | "payment",
  monthLabel: string
): { readyCount: number; issueCount: number; totalCount: number; issues: ReadinessIssue[] } {
  let readyCount = 0;
  let issueCount = 0;
  const issues: ReadinessIssue[] = [];

  for (const vr of rows) {
    const checks = getReadinessChecks(vr, mode, monthLabel);
    if (isExcludedFromIssue(checks)) continue;

    if (isReadyForIssue(checks)) {
      readyCount += 1;
    } else {
      issueCount += 1;
      const context =
        mode === "billing"
          ? `${vr.row.client_name} · ${vr.row.project_name}`
          : `${vr.row.partner_name || "自社"} · ${vr.row.project_name}`;

      for (const check of checks) {
        if (check.status === "missing" || check.status === "warning") {
          issues.push({
            engineerName: vr.row.engineer_name,
            context,
            check,
          });
        }
      }
    }
  }

  return {
    readyCount,
    issueCount,
    totalCount: readyCount + issueCount,
    issues,
  };
}
