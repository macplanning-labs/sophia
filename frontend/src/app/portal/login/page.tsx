"use client";

import { useState } from "react";

export default function PortalLoginPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
      const res = await fetch("/api/v1/auth/portal-login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ email }),
      });
      const data = await res.json();
      if (data.success) {
        setSent(true);
      } else {
        setError(data.error || "エラーが発生しました");
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
          <h1 className="text-3xl font-bold bg-gradient-to-r from-emerald-400 to-teal-400 bg-clip-text text-transparent">
            Sophia
          </h1>
          <p className="text-muted-foreground text-sm mt-1">パートナーポータル</p>
        </div>

        {/* ログインカード */}
        <div className="bg-background/60 border border-border rounded-xl p-8 backdrop-blur-sm shadow-2xl">
          {sent ? (
            /* 送信完了 */
            <div className="text-center space-y-4">
              <div className="text-5xl">✉️</div>
              <h2 className="text-lg font-semibold text-foreground">メールを確認してください</h2>
              <p className="text-muted-foreground text-sm leading-relaxed">
                <span className="text-foreground font-medium">{email}</span> にログインリンクを送信しました。
                <br />
                メール内のリンクをクリックするとログインできます。
              </p>
              <div className="pt-4 border-t border-border mt-6">
                <p className="text-muted-foreground text-xs mb-3">メールが届かない場合</p>
                <button
                  onClick={() => { setSent(false); setEmail(""); }}
                  className="text-primary hover:text-primary/80 text-sm font-medium transition-colors"
                >
                  もう一度送信する
                </button>
              </div>
            </div>
          ) : (
            /* メールアドレス入力 */
            <>
              <h2 className="text-lg font-semibold text-foreground mb-2">ログイン</h2>
              <p className="text-muted-foreground text-sm mb-6">
                メールアドレスを入力すると、ログインリンクが届きます。
                <br />
                パスワードは不要です。
              </p>

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
                    className="w-full px-3 py-2.5 bg-muted border border-border rounded-lg text-foreground placeholder-muted-foreground/50 focus:outline-none focus:ring-2 focus:ring-emerald-500/50 focus:border-emerald-500 transition-all"
                    placeholder="your@email.com"
                  />
                </div>
                <button
                  type="submit"
                  disabled={loading}
                  className="w-full py-2.5 bg-gradient-to-r from-emerald-500 to-teal-500 hover:from-emerald-400 hover:to-teal-400 disabled:opacity-50 text-white font-medium rounded-lg transition-all duration-200 shadow-lg shadow-emerald-500/25"
                >
                  {loading ? "送信中..." : "ログインリンクを送信"}
                </button>
              </form>
            </>
          )}
        </div>

        <p className="text-center text-muted-foreground text-xs mt-6">
          © 2026 Sophia. All rights reserved.
        </p>
      </div>
    </div>
  );
}
