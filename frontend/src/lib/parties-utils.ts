/**
 * parties-utils.ts — 取引先画面の共通関数
 */

/**
 * 請求単位を日本語で表示
 * @param value 請求単位コード ("PROJECT" | "CLIENT" | undefined)
 * @returns 日本語表示
 */
export function formatBillingUnit(value: string | undefined): string {
  if (!value) return "";
  if (value === "PROJECT") return "案件ごと";
  if (value === "CLIENT") return "取引先まとめ";
  return value;
}
