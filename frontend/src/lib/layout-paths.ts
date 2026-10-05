// ヘッダー・サイドバーを出さないページ(ログイン、MFA、パートナーポータル、公開トークンページ)。
// サイドバーとヘッダーで二重に定義されていたものを、ここに集約する。
export const CHROME_HIDDEN_PATHS = ["/login", "/mfa", "/portal", "/upload", "/invite", "/token"];

export function isChromeHidden(pathname: string): boolean {
  return CHROME_HIDDEN_PATHS.some((p) => pathname.startsWith(p));
}
