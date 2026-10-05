// アサイン編成の入力フォーム(サイドパネル)の、既定値・API 用の変換・入力検証(画面から切り出した純関数)。
// 金額・粗利はサーバー(アサイン試算API)の結果を表示するので、ここでは計算しない。入力が揃うまで試算を呼ばないための判定だけを持つ。

export type StaffType = "EMPLOYEE" | "PARTNER";

/** 単価と精算幅(受注側・発注側で同じ形)。入力欄の文字列のまま持つ */
export interface TermsFields {
  base_rate: string;
  effort: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  deduction_rate: string;
  overtime_rate: string;
}

export interface AssignmentForm {
  project_id: string;
  engineer_id: number;
  staff_type: StaffType;
  start_date: string; // YYYY-MM-DD
  end_date: string;
  client: TermsFields;
  /** パートナー要員のときだけ */
  partner: (TermsFields & { partner_id: string; work_location_name: string }) | null;
}

export interface EngineerLite {
  id: number;
  name: string;
  affiliation_type: string;
  partner_id: string | null;
}

const pad = (n: number) => String(n).padStart(2, "0");
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** 既定の期間: 来月1日 〜 その6か月後の月末 */
export function defaultPeriod(today: Date): { start: string; end: string } {
  const start = new Date(today.getFullYear(), today.getMonth() + 1, 1);
  const end = new Date(start.getFullYear(), start.getMonth() + 6, 0); // 6か月後の月の0日 = 来月から数えて6か月目の月末
  return { start: ymd(start), end: ymd(end) };
}

const emptyTerms = (): TermsFields => ({
  base_rate: "",
  effort: "1",
  lower_limit_hours: "140",
  upper_limit_hours: "180",
  deduction_rate: "0",
  overtime_rate: "0",
});

/** 要員を案件へドロップ(選択)したときの、フォームの初期値 */
export function initialForm(engineer: EngineerLite, projectId: string, today: Date): AssignmentForm {
  const { start, end } = defaultPeriod(today);
  const staff_type: StaffType = engineer.affiliation_type === "PARTNER" ? "PARTNER" : "EMPLOYEE";
  return {
    project_id: projectId,
    engineer_id: engineer.id,
    staff_type,
    start_date: start,
    end_date: end,
    client: emptyTerms(),
    partner:
      staff_type === "PARTNER"
        ? { ...emptyTerms(), partner_id: engineer.partner_id ?? "", work_location_name: "" }
        : null,
  };
}

const num = (s: string): number | null => {
  if (s.trim() === "") return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
};

function termsBody(t: TermsFields) {
  return {
    base_rate: num(t.base_rate),
    effort: num(t.effort),
    lower_limit_hours: num(t.lower_limit_hours),
    upper_limit_hours: num(t.upper_limit_hours),
    deduction_rate: num(t.deduction_rate) ?? 0,
    overtime_rate: num(t.overtime_rate) ?? 0,
  };
}

/** 試算・作成に必要な数値が揃っているか(揃うまでは試算APIを呼ばない) */
function termsReady(t: TermsFields): boolean {
  const b = termsBody(t);
  return b.base_rate !== null && b.base_rate > 0 && b.effort !== null && b.effort > 0 &&
    b.lower_limit_hours !== null && b.upper_limit_hours !== null;
}

/** 入力の検証(サーバーと同じ規則)。問題があれば画面に出せる日本語、なければ null */
export function validateForm(f: AssignmentForm): string | null {
  if (!f.start_date || !f.end_date) return "期間を入力してください";
  if (f.end_date < f.start_date) return "終了日は、開始日以後の日付にしてください";
  const check = (t: TermsFields, label: string): string | null => {
    const b = termsBody(t);
    if (b.base_rate === null || b.base_rate <= 0) return `${label}の単価を1円以上で入力してください`;
    if (b.effort === null || b.effort <= 0) return `${label}の工数は0より大きい値で入力してください`;
    if (b.lower_limit_hours === null || b.upper_limit_hours === null) return `${label}の精算幅を入力してください`;
    if (b.lower_limit_hours < 0 || b.upper_limit_hours < b.lower_limit_hours) {
      return `${label}の精算幅は、下限が0以上で、上限が下限以上になるよう入力してください`;
    }
    if (b.deduction_rate < 0 || b.overtime_rate < 0) return `${label}の控除・超過の単価は0以上で入力してください`;
    return null;
  };
  const c = check(f.client, "受注");
  if (c) return c;
  if (f.staff_type === "PARTNER") {
    if (!f.partner) return "提案元パートナー(発注)が必要です";
    if (!f.partner.partner_id.trim()) return "提案元パートナーを選択してください";
    const p = check(f.partner, "発注");
    if (p) return p;
  }
  return null;
}

/** 利益試算APIのリクエスト。入力が揃っていなければ null(呼ばない) */
export function toPreviewRequest(f: AssignmentForm) {
  if (!termsReady(f.client)) return null;
  if (f.staff_type === "PARTNER" && (!f.partner || !termsReady(f.partner))) return null;
  return {
    project_id: f.project_id,
    engineer_id: f.engineer_id,
    client_contract: termsBody(f.client),
    partner_contract: f.staff_type === "PARTNER" && f.partner ? termsBody(f.partner) : undefined,
  };
}

/** アサイン作成APIのリクエスト */
export function toCreateRequest(f: AssignmentForm) {
  return {
    project_id: f.project_id,
    staff_type: f.staff_type,
    engineer_id: f.engineer_id,
    client_contract: { start_date: f.start_date, end_date: f.end_date, ...termsBody(f.client) },
    partner_contract:
      f.staff_type === "PARTNER" && f.partner
        ? {
            partner_id: f.partner.partner_id,
            start_date: f.start_date,
            end_date: f.end_date,
            work_location_name: f.partner.work_location_name || undefined,
            ...termsBody(f.partner),
          }
        : undefined,
  };
}

/** 試算APIを呼び直すかの判定に使う、入力の指紋(同じ入力なら同じ文字列) */
export function previewKey(f: AssignmentForm): string | null {
  const r = toPreviewRequest(f);
  return r ? JSON.stringify(r) : null;
}
