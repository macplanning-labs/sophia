/** 契約期間内の対象月（YYYY-MM）候補を生成 */
export function monthsInRange(startDate: string, endDate: string): { value: string; label: string }[] {
  if (!startDate || !endDate) return [];
  const start = new Date(`${startDate.slice(0, 7)}-01T00:00:00`);
  const end = new Date(`${endDate.slice(0, 7)}-01T00:00:00`);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime()) || start > end) return [];
  const out: { value: string; label: string }[] = [];
  const cur = new Date(start);
  while (cur <= end) {
    const y = cur.getFullYear();
    const m = String(cur.getMonth() + 1).padStart(2, "0");
    out.push({ value: `${y}-${m}`, label: `${y}年${m}月` });
    cur.setMonth(cur.getMonth() + 1);
  }
  return out;
}

/** 今月が範囲内なら今月、なければ最終月 */
export function defaultMonthValue(startDate: string, endDate: string): string {
  const options = monthsInRange(startDate, endDate);
  if (options.length === 0) return "";
  const now = new Date();
  const current = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  if (options.some((o) => o.value === current)) return current;
  return options[options.length - 1].value;
}

/** 対象月と契約期間から target_month / work_start / work_end を算出 */
export function buildWorkPeriod(yearMonth: string, contractStart: string, contractEnd: string) {
  const [y, m] = yearMonth.split("-").map(Number);
  const targetMonth = `${yearMonth}-01`;
  const lastDay = new Date(y, m, 0).getDate();
  const monthEnd = `${yearMonth}-${String(lastDay).padStart(2, "0")}`;
  const workStart = contractStart > targetMonth ? contractStart : targetMonth;
  const workEnd = contractEnd < monthEnd ? contractEnd : monthEnd;
  return { target_month: targetMonth, work_start: workStart, work_end: workEnd };
}
