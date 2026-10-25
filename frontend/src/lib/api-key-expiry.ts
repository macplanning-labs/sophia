/** APIキーの有効期限の表示。未設定は無期限、過ぎていれば「期限切れ」 (DEMO-000167 項目7) */
export function formatExpiry(expiresAt?: string | null, now: number = Date.now()): string {
  if (!expiresAt) return "無期限";
  const t = new Date(expiresAt);
  if (Number.isNaN(t.getTime())) return "期限: 不明";
  const date = t.toLocaleDateString("ja-JP");
  return t.getTime() <= now ? `期限切れ (${date})` : `期限: ${date}`;
}
