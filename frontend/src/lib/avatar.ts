// アカウントメニューのアバターに出す1文字。氏名があれば氏名の頭文字(例: 佐藤次郎 → 吉)、無ければ null(人型のアイコンを出す)。
// メールアドレスの頭文字は、誰のものか分かりにくいので使わない。
export function avatarLabel(displayName: string | null | undefined): string | null {
  const name = (displayName ?? "").trim();
  if (!name) return null;
  const first = Array.from(name)[0]; // サロゲートペアも1文字として数える
  return first.toUpperCase();
}
