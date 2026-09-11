import { type GuidanceItem } from "./types";

/** 発注書詳細のステータスに応じた次アクション */
export function getPurchaseOrderStatusGuidance(
  status: string,
  orderId: string
): GuidanceItem | null {
  switch (status) {
    case "DRAFT":
      return {
        title: "次のステップ",
        message:
          "「送付」からパートナーへ発注書を送り、承諾（受諾済）まで進めてください。承諾後に月次確定から支払通知を発行できます。",
        linkPath: `/orders/${orderId}`,
        linkLabel: "この画面で送付",
      };
    case "SENT":
      return {
        title: "パートナー承諾待ち",
        message:
          "パートナーが発注書を承諾するまでお待ちください。承諾後は月次確定の支払タブから支払通知を発行できます。",
        linkPath: "/settlement",
        linkLabel: "月次確定を開く",
      };
    default:
      return null;
  }
}

/** 請求書詳細のステータスに応じた次アクション */
export function getInvoiceStatusGuidance(status: string): GuidanceItem | null {
  switch (status) {
    case "PENDING_APPROVAL":
      return {
        title: "承認が必要です",
        message:
          "内容を確認し「承認」してください。承認後にクライアントへ送付できます。",
      };
    case "APPROVED":
      return {
        message:
          "承認済みです。「送信メール確認・送信」からクライアントへ請求書を送付してください。",
      };
    default:
      return null;
  }
}

/** 受注書で契約未紐付け時（L2） */
export function getReceivedOrderContractGuidance(): GuidanceItem {
  return {
    title: "受注契約が未紐付けです",
    message:
      "EDI・クロス等の注文書は取込時に契約を特定できない場合があります。稼働報告・進捗連携のため、下の一覧から受注契約を紐付けてください。件名がマスタと違う場合は案件の「EDI案件別名」も設定してください。",
    linkPath: "/client-contracts",
    linkLabel: "受注契約一覧を開く",
  };
}
