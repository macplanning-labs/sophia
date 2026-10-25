// lib/sso-login.ts — 社内 SSO(Cloudflare Access)でのログインの呼び出し(SOSSO F1 / DEMO-000237)
//
// ログイン画面が、社内アカウントでのログインを試すための部品。
// - 画面を開いたとき、SSO が有効なら自動で 1 回だけ試す(失敗・無効のときは、そのタブでは試し直さない)
// - ログアウトした直後は、自動では試さない(すぐ入り直してしまうため。ボタンで戻れる)
// - 直近 60 秒以内に自動で試していたら、自動では試さない(ログイン → 画面 → ログインへ戻る、の往復を止める)
//   ※ SENN で、未ログインの取得が失敗してログイン画面が無限に読み込み直される不具合があった(d0a0a3cd)。同じ形を防ぐ。
// 理由(アカウントなし・社外の人・検証失敗)は区別せず、同じ文言にする(アカウントの有無を、外から推測させない)。

/**
 * 失敗の種類。
 * - unauthorized: Cloudflare Access を、まだ通っていない(初めて・期限切れ)。エラーではない。ボタンで入口へ進めばよい。
 * - denied: Access は通ったが、入れるアカウントではない(アカウントなし・社外の人)。理由は出さない。
 * - error: サーバーの失敗・通信エラー。
 */
export type SsoFailureReason = "unauthorized" | "denied" | "error";

export type SsoResult =
  | { status: "loggedIn"; role: string }
  | { status: "disabled" }
  | { status: "failed"; reason: SsoFailureReason; message: string };

export const SSO_DENIED_MESSAGE =
  "社内アカウントでログインできませんでした。従来のログインをお使いください";

const JUST_LOGGED_OUT_KEY = "sophia_just_logged_out";
const AUTO_ATTEMPT_KEY = "sophia_sso_auto_attempt_at";
/** 自動で試す最短の間隔(ミリ秒) */
export const AUTO_ATTEMPT_INTERVAL_MS = 60_000;

function storage(): Storage | null {
  try {
    return typeof window === "undefined" ? null : window.sessionStorage;
  } catch {
    // プライベートウィンドウなどで使えないときは、無いものとして扱う(画面は動く)
    return null;
  }
}

/** ログアウトの直前に呼ぶ。次にログイン画面を開いたとき、自動では試さない。 */
export function markJustLoggedOut(): void {
  try {
    storage()?.setItem(JUST_LOGGED_OUT_KEY, "1");
  } catch {
    // 保存できなくても、ログアウトは続ける
  }
}

/** ログアウト直後かを返し、印を消す(1 回だけ有効)。 */
export function consumeJustLoggedOut(): boolean {
  const s = storage();
  if (!s) return false;
  try {
    const was = s.getItem(JUST_LOGGED_OUT_KEY) === "1";
    s.removeItem(JUST_LOGGED_OUT_KEY);
    return was;
  } catch {
    return false;
  }
}

/** 自動で試してよいか(直近に自動で試していなければ true)。 */
export function canAutoAttempt(now: number = Date.now()): boolean {
  // 抑止の記録を持てないときは、往復を防ぐため、自動では試さない(ボタンでの入り方は変えない)
  const s = storage();
  if (!s) return false;
  try {
    const last = Number(s.getItem(AUTO_ATTEMPT_KEY));
    return !(Number.isFinite(last) && last > 0 && now - last < AUTO_ATTEMPT_INTERVAL_MS);
  } catch {
    return false;
  }
}

/** 自動で試す直前に呼ぶ(時刻を残す)。 */
export function recordAutoAttempt(now: number = Date.now()): void {
  try {
    storage()?.setItem(AUTO_ATTEMPT_KEY, String(now));
  } catch {
    // 残せなくても続ける
  }
}

/** 社内 SSO が有効なサーバーか(有効かどうかの真偽だけを返す問い合わせ)。失敗したら false(従来のログインだけ)。 */
export async function fetchSsoEnabled(): Promise<boolean> {
  try {
    const res = await fetch("/api/v1/auth/sso/status", { credentials: "include" });
    if (!res.ok) return false;
    const data = await res.json();
    return data?.enabled === true;
  } catch {
    return false;
  }
}

/** 社内アカウントでのログインを試す。成功するとログインの Cookie が付く(画面は遷移だけすればよい)。 */
export async function trySsoLogin(): Promise<SsoResult> {
  let res: Response;
  try {
    res = await fetch("/api/v1/auth/sso/login", {
      method: "POST",
      credentials: "include",
    });
  } catch {
    return { status: "failed", reason: "error", message: "サーバーに接続できません" };
  }
  // 404 = このサーバーでは SSO が無効(画面に SSO を出さない)
  if (res.status === 404) return { status: "disabled" };

  let data: { success?: boolean; role?: string; error?: string } = {};
  try {
    data = await res.json();
  } catch {
    // 本文が JSON でなくても、状態コードで判断する
  }
  if (res.status === 200 && data.success) {
    return { status: "loggedIn", role: data.role ?? "" };
  }
  // 401(Access を通っていない)は、エラーではない。403 は、理由を出さずに同じ文言にする。
  // それ以外(500 など)は、サーバーの文言があれば使う
  if (res.status === 401) {
    return { status: "failed", reason: "unauthorized", message: SSO_DENIED_MESSAGE };
  }
  if (res.status === 403) {
    return { status: "failed", reason: "denied", message: SSO_DENIED_MESSAGE };
  }
  return { status: "failed", reason: "error", message: data.error || SSO_DENIED_MESSAGE };
}

/**
 * Cloudflare Access で保護した入口の URL。ボタンは、ここへ「画面ごと」移動する(`fetch` では、Access のログイン画面への
 * 転送を完了できない)。Access のログイン(Keycloak。障害時は Google)を通ると、サーバーがログインして、`redirect` へ戻す。
 * `redirect` は、サイト内の相対パスだけ(外部サイトなどは、渡さない)。
 */
export function ssoEntryUrl(redirect: string | null | undefined): string {
  const base = "/api/v1/auth/sso/start";
  return redirect && redirect.startsWith("/") && !redirect.startsWith("//")
    ? `${base}?redirect=${encodeURIComponent(redirect)}`
    : base;
}
