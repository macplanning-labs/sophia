/**
 * master-form.ts — マスタ定義(/api/v1/masters/meta)から入力フォームを組み立てる共通関数。
 *
 * マスタ画面と取引先画面(クライアント・パートナー)が、同じ項目定義を使う。
 * 項目はバックエンドの MASTER_TABLES が正本で、ここでは並べ方(区切り)だけを決める。
 */

export interface MasterColumnMeta {
  name: string;
  label: string;
}

export interface MasterFormColumnMeta {
  name: string;
  label: string;
  input_type: string;
  required: boolean;
  is_pk: boolean;
  options?: { value: string; label: string }[];
  fk_table?: string;
  fk_value?: string;
  fk_label?: string;
}

export interface MasterTableMeta {
  key: string;
  label: string;
  category: string;
  pk_column: string;
  list_columns: MasterColumnMeta[];
  form_columns: MasterFormColumnMeta[];
}

/** フォームの入力値(すべて文字列。チェックボックスは "true"/"false") */
export type MasterFormData = Record<string, string>;

/** 詳細APIの値を、フォームの入力値にする。null・未設定は空文字。 */
export function toMasterFormData(cols: MasterFormColumnMeta[], detail?: Record<string, unknown> | null): MasterFormData {
  const fd: MasterFormData = {};
  for (const col of cols) {
    const val = detail?.[col.name];
    fd[col.name] = val === null || val === undefined ? "" : String(val);
  }
  return fd;
}

/** フォームの入力値を、保存APIに送る形にする(チェックボックスは真偽値、数値は数)。 */
export function toMasterPayload(cols: MasterFormColumnMeta[], fd: MasterFormData): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  for (const col of cols) {
    const v = fd[col.name] ?? "";
    if (col.input_type === "checkbox") {
      payload[col.name] = v === "true" || v === "on";
    } else if (col.input_type === "number" && v !== "") {
      payload[col.name] = Number(v);
    } else {
      payload[col.name] = v;
    }
  }
  return payload;
}

/** 必須項目のうち、空のものの表示名。 */
export function missingRequired(cols: MasterFormColumnMeta[], fd: MasterFormData): string[] {
  return cols.filter((c) => c.required && (fd[c.name] ?? "").trim() === "").map((c) => c.label);
}

/** 選択肢の値を表示名にする(選択肢に無ければ値のまま。空は "—")。 */
export function optionLabel(col: MasterFormColumnMeta | undefined, value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  const found = col?.options?.find((o) => o.value === String(value));
  return found ? found.label : String(value);
}

/** 取引先画面で編集するマスタ(マスタ画面のタブには出さない)。値は取引先画面の種別。 */
export const MASTERS_EDITED_ON_PARTIES: Record<string, "client" | "partner"> = {
  clients: "client",
  partners: "partner",
};

export interface FormSection {
  title: string;
  cols: MasterFormColumnMeta[];
}

/** 取引先の編集フォームの区切り。ここに無い項目は最後の「その他」に入る(項目の取りこぼしを防ぐ)。 */
const PARTY_SECTIONS: Record<"client" | "partner", { title: string; names: string[] }[]> = {
  client: [
    { title: "基本情報", names: ["name", "contact_person", "email", "phone", "address"] },
    { title: "請求・送付先", names: ["billing_unit", "invoice_issued_by_client", "invoice_email", "cc_email", "work_report_email"] },
    { title: "EDI", names: ["edi_system_type", "edi_notification_email"] },
    { title: "インボイス・Peppol", names: ["registration_no", "peppol_participant_id"] },
  ],
  partner: [
    { title: "基本情報", names: ["name", "representative_name", "contact_person", "email", "cc", "tel", "postal_code", "address"] },
    { title: "インボイス・Peppol", names: ["registration_no", "peppol_participant_id"] },
    { title: "振込口座", names: ["bank_name", "bank_branch", "account_type", "account_number"] },
  ],
};

/** 取引先(クライアント・パートナー)のフォーム項目を、区切りごとに分ける。 */
export function groupPartyColumns(type: "client" | "partner", cols: MasterFormColumnMeta[]): FormSection[] {
  const byName = new Map(cols.map((c) => [c.name, c]));
  const used = new Set<string>();
  const sections: FormSection[] = [];
  for (const s of PARTY_SECTIONS[type]) {
    const sc = s.names.map((n) => byName.get(n)).filter((c): c is MasterFormColumnMeta => !!c);
    sc.forEach((c) => used.add(c.name));
    if (sc.length > 0) sections.push({ title: s.title, cols: sc });
  }
  const rest = cols.filter((c) => !used.has(c.name));
  if (rest.length > 0) sections.push({ title: "その他", cols: rest });
  return sections;
}

/** 口座番号の項目名(マスタ定義の列名。自社情報・取引先・マスタ共通) */
export const ACCOUNT_NUMBER_FIELD = "account_number";

/**
 * 口座番号で「ハイフン」として扱う文字(サーバ AccountNumber::parse_input と同じ集合。レビュー指摘 3)。
 * `-` `－` `‐`〜`–` `−` に加え、日本語 IME が出す `ー`(U+30FC)・`―`(U+2015)と、幅ゼロの空白(U+200B)。
 */
const ACCOUNT_HYPHEN_CLASS = "\\-\\uFF0D\\u2010-\\u2013\\u2212\\u30FC\\u2015\\u200B";
/** 口座番号で「空白」として扱う文字(半角・全角・NBSP。タブは含めない。サーバと同じ) */
const ACCOUNT_SPACE_CLASS = "\\u0020\\u3000\\u00A0";
const ACCOUNT_HYPHEN_RE = new RegExp(`[${ACCOUNT_HYPHEN_CLASS}]`, "u");
const ACCOUNT_SPACE_RE = new RegExp(`[${ACCOUNT_SPACE_CLASS}]`, "u");
const ACCOUNT_DIGIT_RE = /[0-9\uFF10-\uFF19]/;

/** 口座番号の入力欄の属性(数字の入力に合わせる。全角・ハイフン・空白も打てる。桁数の検査はサーバと同じ規則で行う) */
export const ACCOUNT_NUMBER_INPUT_PROPS = {
  inputMode: "numeric" as const,
  maxLength: 40,
  pattern: `[0-9\\uFF10-\\uFF19${ACCOUNT_HYPHEN_CLASS}${ACCOUNT_SPACE_CLASS}]*`,
  title: "数字で入力してください(ハイフン・空白は使えます)",
};

/**
 * 口座番号の入力の誤りを、利用者向けの文言で返す(誤りが無ければ null)。
 * サーバ(AccountNumber::parse_input)と同じ規則: enc1: で始まる・数字以外・21 桁以上を拒否。
 * 全角数字・ハイフン・空白は受け付ける(保存のときサーバが数字だけに直す)。空は未登録として許す。
 */
export function accountNumberFormError(value: string): string | null {
  if (value.trim().startsWith("enc1:")) return "口座番号の形式が正しくありません";
  let digits = 0;
  let hasHyphen = false;
  for (const ch of Array.from(value)) {
    if (ACCOUNT_DIGIT_RE.test(ch)) {
      digits += 1;
    } else if (ACCOUNT_HYPHEN_RE.test(ch)) {
      hasHyphen = true;
    } else if (!ACCOUNT_SPACE_RE.test(ch)) {
      return "口座番号は数字で入力してください";
    }
  }
  if (digits === 0 && hasHyphen) return "口座番号は数字で入力してください";
  if (digits > 20) return "口座番号は数字 1〜20 桁で入力してください";
  return null;
}

/** 保存の前に、フォームの口座番号の欄だけを検査する(口座番号の欄が無い表では null) */
export function accountNumberColumnError(cols: MasterFormColumnMeta[], fd: MasterFormData): string | null {
  if (!cols.some((c) => c.name === ACCOUNT_NUMBER_FIELD)) return null;
  return accountNumberFormError(fd[ACCOUNT_NUMBER_FIELD] ?? "");
}
