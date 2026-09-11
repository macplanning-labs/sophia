import { type GuidanceItem } from "./types";

export interface SettlementRow {
  timesheet_status?: string;
  invoice_issued?: boolean;
  notice_issued?: boolean;
  partner_contract_id?: number | null;
  engineer_name?: string;
  timesheet_id?: number;
}

export function getUnapprovedTimesheetGuidance(
  row: SettlementRow,
  viewMode: "billing" | "payment",
  monthLabel: string
): GuidanceItem {
  const documentLabel = viewMode === "billing" ? "請求書" : "支払通知";
  const hasTimesheet = !!row.timesheet_id;

  if (hasTimesheet) {
    return {
      message: `${row.engineer_name}さんの${monthLabel}分の稼働報告は提出済みですが、まだ承認されていません。承認すると${documentLabel}を発行できます。`,
      linkPath: `/timesheets?edit=${row.timesheet_id}`,
      linkLabel: "稼働報告の承認画面へ進む",
    };
  } else {
    return {
      message: `${row.engineer_name}さんの${monthLabel}分の稼働報告が未提出です。${documentLabel}を発行するには、稼働報告の登録が必要です。`,
      linkPath: "/timesheets?upload=1",
      linkLabel: "稼働報告の登録画面へ進む",
    };
  }
}

export function getSettlementRowGuidance(
  row: SettlementRow,
  viewMode: "billing" | "payment"
): GuidanceItem | null {
  // 自社エンジニア（支払側で partner_contract_id が null）の場合
  if (viewMode === "payment" && row.partner_contract_id === null) {
    return {
      message: "発注契約を登録すると支払通知の対象になります。",
      linkPath: "/partner-contracts",
      linkLabel: "発注契約を登録",
    };
  }

  // 稼働報告未承認
  if (
    row.timesheet_status &&
    row.timesheet_status !== "APPROVED" &&
    row.timesheet_status !== "SENT"
  ) {
    return {
      message: "稼働報告の承認が必要です。",
      linkPath: "/timesheets?edit",
      linkLabel: "稼働報告を確認",
    };
  }

  return null;
}

export function getSettlementPageGuidance(
  viewMode: "invoice" | "notice"
): GuidanceItem | null {
  if (viewMode === "invoice") {
    return {
      message: "請求 = 受注側（クライアント）へ発行する請求書。対象月の稼働報告が承認済みである必要があります。",
    };
  } else {
    return {
      message: "支払 = 発注側（パートナー）へ発行する支払通知。発注契約と稼働報告の承認が必要です。",
    };
  }
}

export function getSettlementIssuedRowGuidance(
  issued: boolean
): GuidanceItem | null {
  if (issued) {
    return {
      message: "既に発行済みです。再発行は不要です。",
      linkPath: "/invoices",
      linkLabel: "一覧を確認",
    };
  }
  return null;
}
