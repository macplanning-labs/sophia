/**
 * project-wizard.ts — 案件作成ウィザードの判定(画面から切り出した純粋関数)
 */

/** ウィザードの顧客の選択肢(/api/v1/projects/wizard/form-data の clients) */
export interface WizardClientOption {
  value: number;
  label: string;
  /** 担当(組織・担当者)が決まっているか。無い顧客の下には案件を作れない(DEMO-000173) */
  has_owner?: boolean;
}

export const CLIENT_WITHOUT_OWNER_MESSAGE =
  "この取引先は担当が決まっていないため、案件を作れません。設定 > 取引先 で取引先を開き、「担当を変更」で担当を設定してください";

/** 選んだ顧客に担当が無ければ、その案内(選んでいない・担当ありは undefined)。 */
export function clientOwnerProblem(options: WizardClientOption[], clientId: string): string | undefined {
  if (clientId === "") return undefined;
  const c = options.find((o) => String(o.value) === clientId);
  return c && c.has_owner === false ? CLIENT_WITHOUT_OWNER_MESSAGE : undefined;
}
