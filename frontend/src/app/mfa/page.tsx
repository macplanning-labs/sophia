"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useQueryClient } from "@tanstack/react-query";

export default function MfaPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleTotpSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      const res = await fetch("/api/v1/mfa/verify", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ code }),
      });
      const data = await res.json();
      if (data.success) {
        queryClient.clear();
        router.push("/");
      } else {
        setError(data.error || "認証に失敗しました");
      }
    } catch {
      setError("サーバーに接続できません");
    } finally {
      setLoading(false);
    }
  };

  const handlePasskeyAuth = async () => {
    setError("");
    try {
      const beginRes = await fetch("/api/v1/mfa/passkey/begin", {
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

      const { startAuthentication } = await import("@simplewebauthn/browser");
      const credential = await startAuthentication({ optionsJSON });

      const completeRes = await fetch("/api/v1/mfa/passkey/complete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify(credential),
      });

      if (completeRes.ok) {
        queryClient.clear();
        router.push("/");
      } else {
        const text = await completeRes.text();
        setError(text || "パスキー認証に失敗しました");
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : "パスキー認証に失敗しました";
      if (/AbortError|NotAllowedError|取消|canceled|cancelled/i.test(msg)) {
        setError("パスキー認証がキャンセルされました");
      } else {
        setError(msg);
      }
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
          <p className="text-muted-foreground text-sm mt-1">二要素認証</p>
        </div>

        {/* MFA検証カード */}
        <div className="bg-background/60 border border-border rounded-xl p-8 backdrop-blur-sm shadow-2xl">
          <h2 className="text-lg font-semibold text-foreground mb-2">認証コードを入力</h2>
          <p className="text-sm text-muted-foreground mb-6">
            認証アプリに表示されている6桁のコードを入力してください。
          </p>

          {error && (
            <div className="mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
              {error}
            </div>
          )}

          <form onSubmit={handleTotpSubmit} className="space-y-4">
            <div>
              <input
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                maxLength={6}
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
                autoFocus
                className="w-full px-4 py-3 bg-muted border border-border rounded-lg text-foreground text-center text-2xl tracking-[0.5em] font-mono placeholder-muted-foreground/50 focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                placeholder="000000"
              />
            </div>
            <button
              type="submit"
              disabled={loading || code.length !== 6}
              className="w-full py-2.5 bg-gradient-to-r from-primary to-primary/80 hover:from-blue-500 hover:to-indigo-500 disabled:opacity-50 text-foreground font-medium rounded-lg transition-all duration-200 shadow-lg shadow-primary/25"
            >
              {loading ? "検証中..." : "認証する"}
            </button>
          </form>

          {/* パスキー認証 */}
          <div className="mt-6 pt-4 border-t border-border">
            <button
              type="button"
              onClick={handlePasskeyAuth}
              className="w-full py-2.5 border border-border hover:border-border text-muted-foreground hover:text-foreground rounded-lg transition-all duration-200 text-sm flex items-center justify-center gap-2"
            >
              🔑 パスキーで認証
            </button>
          </div>

          {/* 戻るリンク */}
          <div className="mt-4 text-center">
            <button
              type="button"
              onClick={() => {
                fetch("/api/v1/auth/logout", { method: "POST", credentials: "include" });
                router.push("/login");
              }}
              className="text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              ← ログイン画面に戻る
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
