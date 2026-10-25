/**
 * api-result.ts — APIの応答が成功かどうかの判定(DEMO-000198)
 *
 * サーバの応答は、成功を `success`(多くのAPI)か `ok`(メールの取り込み・受信の接続確認・ロック解除)で返す。
 * 片方だけを見ると、成功しても失敗と表示してしまうので、ここで両方を見る。
 */
export function isApiOk(res: { success?: boolean; ok?: boolean } | null | undefined): boolean {
  if (!res) return false;
  return res.success === true || res.ok === true;
}
