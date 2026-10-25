/**
 * sent-emails.ts — 送信メールの記録の表示用(DEMO-000196)
 */

/** 送信日時(サーバは UTC)を日本時間の「YYYY-MM-DD HH:mm」にする。読めなければそのまま返す */
export function formatSentAt(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const parts = new Intl.DateTimeFormat("ja-JP", {
    timeZone: "Asia/Tokyo",
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false,
  }).formatToParts(d);
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")} ${get("hour")}:${get("minute")}`;
}

/** 宛先・件名に検索語(大文字小文字は区別しない)を含む行だけにする */
export function filterSentEmails<T extends { recipient: string; subject: string }>(rows: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((r) => r.recipient.toLowerCase().includes(q) || r.subject.toLowerCase().includes(q));
}
