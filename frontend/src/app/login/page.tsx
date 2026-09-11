"use client";

import { Suspense, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff } from "lucide-react";
import { isSafeRedirect } from "@/lib/safe-redirect";

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

          {error && (
            <div className="mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
              {error}
            </div>
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
        </div>

        <p className="text-center text-muted-foreground text-xs mt-6">
          © 2026 Sophia. All rights reserved.
        </p>
      </div>
    </div>
  );
}
