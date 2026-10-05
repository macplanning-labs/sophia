// lib/safe-redirect.ts — ログイン後リダイレクト先のオープンリダイレクト対策
//
// セッション期限切れモーダルは元画面のURLを ?redirect= に載せて /login (or /portal/login) へ渡す。
// ログイン成功後にそれをそのまま遷移先として使うと、redirect=https://evil.example.com や
// redirect=//evil.example.com のような外部サイトへの誘導に悪用できてしまうため、
// 「サイト内の相対パスであること」を確認してからのみ使用する。

/** 自サイト内の相対パスとして安全に遷移可能かどうかを判定する */
export function isSafeRedirect(path: string | null | undefined): path is string {
  if (!path) return false;
  // "/" 始まりのみ許可。"//evil.com" はプロトコル相対URLとして外部遷移になるため除外。
  return path.startsWith("/") && !path.startsWith("//");
}
