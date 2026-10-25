import type { InvoiceRow, StatusMatrix } from "@/lib/types";

export type CardTone = "bad" | "warn";

export interface HomeCard {
  key: string;
  title: string;
  count: number;
  description: string;
  href: string;
  tone: CardTone;
}

/**
 * ホーム(サイドバー > ホーム > 要対応)の「要対応」カード。月別状況・請求書・受信メールの既存データから数えるだけで、
 * 画面ごとに数字がずれないようにする。0件の種類は出さない。
 */
export function buildHomeCards(
  matrix: StatusMatrix | undefined,
  invoices: InvoiceRow[],
  reviewMailCount: number,
  month: string,
  /** 請求書の確定待ち = いま確定できる請求書の数(請求書の確定の画面の「確定できます」と同じ) */
  confirmableInvoices: number,
): HomeCard[] {
  const rows = matrix?.rows ?? [];
  const n = (pick: (r: StatusMatrix["rows"][number]) => boolean) => rows.filter(pick).length;
  const pendingInvoices = invoices.filter(
    (i) => i.status === "PENDING_APPROVAL" && (i.target_month ?? "").startsWith(month),
  ).length;

  const all: HomeCard[] = [
    { key: "ts-returned", title: "勤務表", count: n((r) => r.timesheet.label === "差し戻し"), description: "差し戻し中(再提出の待ち)", href: `/timesheet-matching?month=${month}`, tone: "bad" },
    { key: "ts-review", title: "勤務表", count: n((r) => r.timesheet.label === "確認待ち"), description: "確認待ち(承認していない)", href: `/timesheet-matching?month=${month}`, tone: "warn" },
    { key: "inv-confirm", title: "請求書", count: confirmableInvoices, description: "確定待ち(確定できる請求書)", href: "/billing", tone: "warn" },
    { key: "inv-approve", title: "請求書", count: pendingInvoices, description: "承認待ち(確定済み)", href: "/invoices", tone: "warn" },
    { key: "notice", title: "支払通知", count: n((r) => r.notice.label === "未作成"), description: "未作成", href: "/settlement", tone: "warn" },
    { key: "po", title: "発注書", count: n((r) => r.purchase_order.label === "未送付"), description: "未送付", href: "/partner-contracts", tone: "warn" },
    { key: "ro", title: "受注書", count: n((r) => r.received_order.label === "注文書なし"), description: "注文書がない(請求できない)", href: "/client-contracts", tone: "bad" },
    { key: "mail", title: "受信メール", count: reviewMailCount, description: "要確認", href: "/received-emails", tone: "warn" },
  ];
  return all.filter((c) => c.count > 0);
}
