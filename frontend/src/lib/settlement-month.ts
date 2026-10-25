/**
 * settlement-month.ts — 月次確定の対象月の読み取り(DEMO-000202)
 */

/** "2026-09" / "2026-09-01" を "2026-09" にする。読めなければ null */
export function monthFromParam(value: string | null | undefined): string | null {
  if (!value) return null;
  const m = value.match(/^(\d{4})-(\d{2})(?:-\d{2})?$/);
  if (!m) return null;
  const month = Number(m[2]);
  if (month < 1 || month > 12) return null;
  return `${m[1]}-${m[2]}`;
}
