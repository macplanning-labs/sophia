import type { StatusNextAction, StatusTone } from "@/lib/types";

/** 次の操作の表示名と移動先(月別状況から、該当する画面へ移る) */
export const NEXT_ACTION: Record<StatusNextAction, { label: string; href: string }> = {
  REGISTER_ORDER: { label: "受注書を登録", href: "/client-contracts" },
  REVIEW_TIMESHEET: { label: "勤務表を確認", href: "/timesheet-matching" },
  REREQUEST_TIMESHEET: { label: "再提出を依頼", href: "/timesheet-matching" },
  SEND_PURCHASE_ORDER: { label: "発注書を送付", href: "/partner-contracts" },
  REVIEW_INVOICE: { label: "請求書を確認", href: "/billing" },
  // 支払通知書を発行できるのは、月次確定の支払タブだけ(/notices は一覧で、作成できない。DEMO-000189)
  CREATE_NOTICE: { label: "支払通知を作成", href: "/settlement?view=payment" },
};

/** 状態バッジの色(Midnight テーマ) */
export const TONE_CLASS: Record<StatusTone, string> = {
  ok: "border-emerald-500/40 text-emerald-400",
  warn: "border-amber-500/40 text-amber-400",
  bad: "border-rose-500/40 text-rose-400",
  na: "border-border text-muted-foreground opacity-60",
};
