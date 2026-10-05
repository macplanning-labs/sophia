/**
 * lib/member-utils.ts — 要員（Members/Engineers）の分類ロジック
 *
 * 分類ルール:
 * - 自社社員 (EMPLOYEE): affiliation_type が "EMPLOYEE" または "INTERNAL"
 * - パートナー要員 (PARTNER): affiliation_type が "PARTNER"
 */

import type { ClientContractRow, ContractRow } from "./types";

/**
 * 要員の区分を判定する純関数
 *
 * @param affiliation_type - Engineer.affiliation_type ("EMPLOYEE", "INTERNAL", "PARTNER")
 * @returns "自社社員" または "パートナー要員"
 */
export function getMemberClassification(affiliation_type: string): "自社社員" | "パートナー要員" {
  if (affiliation_type === "EMPLOYEE" || affiliation_type === "INTERNAL") {
    return "自社社員";
  }
  return "パートナー要員";
}

/**
 * 本日が期間内の受注契約の数を計算する
 *
 * @param engineerId - 要員ID
 * @param clientContracts - 全受注契約一覧
 * @returns 本日が期間内の受注契約数
 */
export function calculateAssignmentCount(
  engineerId: number,
  clientContracts: ClientContractRow[]
): number {
  const today = new Date().toISOString().split("T")[0];
  return clientContracts.filter(
    (c) =>
      c.engineer_id === engineerId &&
      c.start_date <= today &&
      today <= c.end_date
  ).length;
}

/**
 * パートナー要員の提案元パートナーを集計する
 *
 * @param engineerName - 要員名
 * @param partnerContracts - 全発注契約一覧
 * @returns 本日が期間内の発注契約のパートナー名（カンマ区切り）
 */
export function getProposalPartners(
  engineerName: string,
  partnerContracts: ContractRow[]
): string {
  const today = new Date().toISOString().split("T")[0];
  const partners = partnerContracts
    .filter(
      (c) =>
        c.engineer_name === engineerName &&
        c.start_date <= today &&
        today <= c.end_date
    )
    .map((c) => c.partner_name);

  // 重複を除去してカンマ区切りで返す
  const uniquePartners = Array.from(new Set(partners));
  return uniquePartners.join(", ");
}
