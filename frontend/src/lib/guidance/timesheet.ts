import { type GuidanceItem } from "./types";

export function getTimesheetStatusGuidance(
  status: string,
  _context?: { timesheetId?: number; targetMonth?: string }
): GuidanceItem | null {
  if (status === "APPROVED" || status === "SENT") {
    return {
      title: "修正は再アップロードで",
      message: "内容を修正する場合は、一覧の「アップロード」から同じ月のファイルを再提出してください。再承認後に請求・支払に反映されます。",
      linkPath: "/timesheets?upload=1",
      linkLabel: "アップロード画面へ",
    };
  }
  return null;
}

export function getTimesheetUploadPanelGuidance(): GuidanceItem {
  return {
    message:
      "同じ月・同じ契約のファイルを再アップロードすると、既存の稼働報告を上書き更新できます。承認済みの場合も再承認が必要です。",
  };
}

export function getTimesheetRowGuidance(
  status: string,
  _targetMonth?: string
): GuidanceItem | null {
  if (status === "APPROVED" || status === "SENT") {
    return {
      message: "修正は同じ月のファイルを再アップロード。再承認後に反映されます。",
      linkPath: "/timesheets?upload=1",
      linkLabel: "アップロード",
    };
  }
  return null;
}
