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

/**
 * 当社の営業担当（担当組織・担当者）を一覧用の1行にする。例: "営業1課 / 山田 太郎"。
 * 片方だけならその名前、どちらも無ければ "未設定"。
 */
export function formatOwner(orgName?: string | null, employeeName?: string | null): string {
  const parts = [orgName, employeeName].map((v) => (v || "").trim()).filter((v) => v !== "");
  return parts.length === 0 ? "未設定" : parts.join(" / ");
}

/** クライアント登録フォームの入力（担当の選択値は select の value＝文字列。未選択は空文字）。 */
export interface ClientCreateForm {
  name: string;
  contact_person: string;
  email: string;
  billing_unit: string;
}

/**
 * クライアント登録の担当（組織・担当者）の入力チェック。
 * 担当は必須（この取引先の案件・契約・受注・請求書などの担当になる）。問題が無ければ undefined。
 */
export function validateClientOwner(orgId: string, employeeId: string): string | undefined {
  if (!orgId && !employeeId) return "担当の組織と担当者を選んでください";
  if (!orgId) return "担当の組織を選んでください";
  if (!employeeId) return "担当者を選んでください";
  return undefined;
}

/** クライアント登録API（POST /api/v1/clients）に送る本文を組み立てる。 */
export function buildClientCreatePayload(form: ClientCreateForm, orgId: string, employeeId: string): Record<string, unknown> {
  return {
    name: form.name.trim(),
    contact_person: form.contact_person || null,
    email: form.email || null,
    billing_unit: form.billing_unit,
    owner_org_unit_id: Number(orgId),
    owner_employee_id: Number(employeeId),
  };
}

/** 取引先画面の URL。open を渡すと、その取引先の編集を開く。 */
export function partiesHref(type: "client" | "partner", open?: string | number | null): string {
  const q = new URLSearchParams({ type });
  if (open !== null && open !== undefined && String(open) !== "") q.set("open", String(open));
  return `/parties?${q.toString()}`;
}

/** 種別を切り替えた URL(ほかのパラメータは残し、開いていた取引先 ?open= は外す)。 */
export function withPartyType(current: URLSearchParams | { toString(): string }, type: "client" | "partner"): string {
  const q = new URLSearchParams(current.toString());
  q.set("type", type);
  q.delete("open");
  return `?${q.toString()}`;
}
