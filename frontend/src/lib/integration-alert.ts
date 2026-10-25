/** 画面の上部の赤い帯の文(DEMO-000216)。問題が無ければ null(帯は出さない)。 */
export interface AlertSummary {
  title: string;
  detail: string;
}

export function summarizeAlert(problems: string[], checkedAt: string | null): AlertSummary | null {
  if (problems.length === 0) return null;
  const first = problems[0];
  const rest = problems.length - 1;
  const checked = checkedAt ? ` ・ 最後の点検 ${checkedAt}` : "";
  return {
    title: `外部連携に問題があります(${problems.length}件)`,
    detail: `${first}${rest > 0 ? ` ほか ${rest}件` : ""}${checked}`,
  };
}
