"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff } from "lucide-react";
import { isSafeRedirect } from "@/lib/safe-redirect";
import {
  canAutoAttempt,
  consumeJustLoggedOut,
  fetchSsoEnabled,
  recordAutoAttempt,
  SSO_DENIED_MESSAGE,
  ssoEntryUrl,
  trySsoLogin,
} from "@/lib/sso-login";

export default function LoginPage() {
  return (
    <Suspense fallback={null}>
      <LoginForm />
    </Suspense>
  );
}

function LoginForm() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const searchParams = useSearchParams();
  const redirectParam = searchParams.get("redirect");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  // ── 社内アカウント(SSO)でのログイン(SOSSO F1)──
  // ssoEnabled: このサーバーで SSO が有効か / ssoChecking: 確認中 / ssoFailed: 直前の確認が失敗した
  const [ssoEnabled, setSsoEnabled] = useState(false);
  const [ssoChecking, setSsoChecking] = useState(false);
  const [ssoFailed, setSsoFailed] = useState(false);
  // 自動で試すのは、画面を開いたときの 1 回だけ(開発時の二重実行でも 1 回)
  const ssoStarted = useRef(false);

  // 自動の確認(すでに Cloudflare Access を通っているブラウザなら、画面を動かさずにログインできる)。
  // Access をまだ通っていない(401)のは、エラーではない。何も出さず、ボタン(入口へ移動)を出す。
  const runSilentSso = async () => {
    setError("");
    setSsoFailed(false);
    setSsoChecking(true);
    const result = await trySsoLogin();
    setSsoChecking(false);
    if (result.status === "loggedIn") {
      // 前のアカウントのキャッシュ(ロール情報・一覧データ等)が残らないよう、従来のログインと同じく消す
      queryClient.clear();
      router.push(isSafeRedirect(redirectParam) ? redirectParam : "/");
    } else if (result.status === "disabled") {
      setSsoEnabled(false);
    } else if (result.reason !== "unauthorized") {
      // Access は通ったが入れない(403)・サーバーの失敗。理由は出さず、従来のフォームと「もう一度」を出す
      setError(result.message);
      setSsoFailed(true);
    }
  };

  // ボタン: 保護した入口へ画面ごと移動する。Access のログインを通ると、サーバーがログインして戻す。
  const goToSsoEntry = () => {
    window.location.assign(ssoEntryUrl(isSafeRedirect(redirectParam) ? redirectParam : null));
  };

  useEffect(() => {
    if (ssoStarted.current) return;
    ssoStarted.current = true;
    (async () => {
      // SSO が無効なサーバー(ローカル開発など)では、何も出さず、何も試さない
      if (!(await fetchSsoEnabled())) return;
      setSsoEnabled(true);
      // 入口(Access のログイン)から、入れずに戻ってきた(?sso_error=1)。理由は出さず、もう一度を選べる。自動では試さない
      if (searchParams.get("sso_error") === "1") {
        setError(SSO_DENIED_MESSAGE);
        setSsoFailed(true);
        return;
      }
      // ログアウト直後・直近に自動で試した直後は、自動では試さない(ボタンで戻れる)
      if (consumeJustLoggedOut() || !canAutoAttempt()) return;
      recordAutoAttempt();
      await runSilentSso();
    })();
    // 画面を開いたときの 1 回だけ。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      const res = await fetch("/api/v1/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ email, password }),
      });
      const data = await res.json();
      if (res.status === 429 && data.error === "account_locked") {
        // attempt_lock_middleware（auth-core）が返す試行回数超過レスポンス。
        // success フィールドは無く、error はコード値のためここで日本語文言に変換する
        const retrySecs = typeof data.retry_after_seconds === "number" ? data.retry_after_seconds : null;
        const retryMin = retrySecs !== null ? Math.ceil(retrySecs / 60) : null;
        setError(
          retryMin !== null
            ? `試行回数が上限に達しました。${retryMin}分ほど時間をおいて再度お試しください`
            : "試行回数が上限に達しました。しばらくしてから再度お試しください"
        );
      } else if (data.success) {
        // ログインはrouter.pushによるSPA遷移（フルリロードなし）のため、同一タブで
        // 別アカウントへログインし直した場合に前のアカウントのキャッシュ（ロール情報・一覧データ等）
        // が残らないよう明示的にクリアする
        queryClient.clear();
        if (data.mfa_required) {
          // MFA検証導線を redirect パラメータで迂回させない
          router.push("/mfa");
        } else if (data.mfa_setup_required) {
          // 社員 MFA 未登録: ダッシュボードAPIの403で /login に戻されないよう直送
          router.push("/settings/security");
        } else if (isSafeRedirect(redirectParam)) {
          // セッション期限切れモーダル経由の再ログイン: 元いた画面に戻す
          router.push(redirectParam);
        } else if (data.role === "ENGINEER" || data.role === "PARTNER") {
          router.push("/portal/timesheet-entry");
        } else {
          router.push("/");
        }
      } else {
        setError(data.error || "ログインに失敗しました");
      }
    } catch {
      setError("サーバーに接続できません");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        {/* ロゴ */}
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold bg-gradient-to-r from-blue-400 to-indigo-400 bg-clip-text text-transparent">
            Sophia
          </h1>
          <p className="text-muted-foreground text-sm mt-1">SES受発注管理システム</p>
        </div>

        {/* ログインカード */}
        <div className="bg-background/60 border border-border rounded-xl p-8 backdrop-blur-sm shadow-2xl">
          <h2 className="text-lg font-semibold text-foreground mb-6">ログイン</h2>

          {/* 社内アカウントを確認している間(フォームは、そのまま使える) */}
          {ssoChecking && (
            <div
              role="status"
              className="mb-4 p-3 bg-primary/10 border border-primary/30 rounded-lg text-primary text-sm flex items-center gap-2"
            >
              <span className="inline-block w-3.5 h-3.5 border-2 border-primary/30 border-t-primary rounded-full animate-spin" />
              社内アカウントを確認しています…
            </div>
          )}

          {error && (
            <div className="mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
              {error}
            </div>
          )}

          {/* 社内アカウントでログイン(社員・管理者向け)。確認中・失敗の直後は出さない(失敗の直後は、下に「もう一度試す」) */}
          {ssoEnabled && !ssoChecking && !ssoFailed && (
            <>
              <button
                type="button"
                onClick={goToSsoEntry}
                className="w-full py-2.5 bg-gradient-to-r from-primary to-primary/80 hover:from-blue-500 hover:to-indigo-500 text-foreground font-medium rounded-lg transition-all duration-200 shadow-lg shadow-primary/25"
              >
                🏢 社内アカウントでログイン
              </button>
              <div className="flex items-center gap-3 my-5 text-xs text-muted-foreground">
                <span className="flex-1 h-px bg-border" />
                または、メールアドレスで
                <span className="flex-1 h-px bg-border" />
              </div>
            </>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm text-muted-foreground mb-1.5">
                メールアドレス
              </label>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                autoFocus
                className="w-full px-3 py-2.5 bg-muted border border-border rounded-lg text-foreground placeholder-muted-foreground/50 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                placeholder="admin@example.com"
              />
            </div>
            <div>
              <label className="block text-sm text-muted-foreground mb-1.5">
                パスワード
              </label>
              <div className="relative">
                <input
                  type={showPassword ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  className="w-full px-3 py-2.5 pr-10 bg-muted border border-border rounded-lg text-foreground placeholder-muted-foreground/50 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                  placeholder="••••••••"
                />
                <button
                  type="button"
                  onClick={() => setShowPassword((v) => !v)}
                  aria-label={showPassword ? "パスワードを隠す" : "パスワードを表示"}
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground transition-colors"
                >
                  {showPassword ? <EyeOff size={18} /> : <Eye size={18} />}
                </button>
              </div>
            </div>
            <button
              type="submit"
              disabled={loading}
              className="w-full py-2.5 bg-gradient-to-r from-primary to-primary/80 hover:from-blue-500 hover:to-indigo-500 disabled:opacity-50 text-foreground font-medium rounded-lg transition-all duration-200 shadow-lg shadow-primary/25"
            >
              {loading ? "認証中..." : "ログイン"}
            </button>
          </form>

          {/* パスキーログイン */}
          <div className="mt-6 pt-4 border-t border-border">
            <button
              type="button"
              onClick={async () => {
                try {
                  const beginRes = await fetch("/api/v1/auth/passkey/login/begin", {
                    method: "POST",
                    credentials: "include",
                  });
                  if (!beginRes.ok) {
                    const text = await beginRes.text();
                    setError(text || "パスキー認証を開始できません");
                    return;
                  }
                  const rcr = await beginRes.json();
                  // webauthn-rs は { publicKey: ... }、simplewebauthn v13 は optionsJSON に中身を渡す
                  const optionsJSON = rcr.publicKey ?? rcr;

                  // WebAuthn API 呼び出し
                  const { startAuthentication } = await import("@simplewebauthn/browser");
                  const credential = await startAuthentication({ optionsJSON });

                  const completeRes = await fetch("/api/v1/auth/passkey/login/complete", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    credentials: "include",
                    body: JSON.stringify(credential),
                  });
                  const result = await completeRes.json();
                  if (result.success) {
                    queryClient.clear();
                    router.push(isSafeRedirect(redirectParam) ? redirectParam : "/");
                  } else {
                    setError(result.error || "パスキー認証に失敗しました");
                  }
                } catch (err) {
                  // eslint-disable-next-line no-console
                  console.error("passkey login failed:", err);
                  const msg = err instanceof Error ? err.message : String(err);
                  if (/AbortError|NotAllowedError|取消|canceled|cancelled/i.test(msg)) {
                    setError("パスキー認証がキャンセルされました");
                  } else {
                    setError(msg || "パスキー認証に失敗しました");
                  }
                }
              }}
              className="w-full py-2.5 border border-border hover:border-border text-muted-foreground hover:text-foreground rounded-lg transition-all duration-200 text-sm flex items-center justify-center gap-2"
            >
              🔑 パスキーでログイン
            </button>
          </div>

          {/* 社内アカウントで入れなかった直後は、もう一度試せる */}
          {ssoEnabled && ssoFailed && !ssoChecking && (
            <div className="mt-3">
              <button
                type="button"
                onClick={goToSsoEntry}
                className="w-full py-2.5 border border-border hover:border-border text-muted-foreground hover:text-foreground rounded-lg transition-all duration-200 text-sm flex items-center justify-center gap-2"
              >
                🏢 社内アカウントでもう一度試す
              </button>
            </div>
          )}
        </div>

        <p className="text-center text-muted-foreground text-xs mt-6">
          © 2026 Sophia. All rights reserved.
        </p>
      </div>
    </div>
  );
}
