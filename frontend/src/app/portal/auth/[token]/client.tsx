"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useDynamicId } from "@/lib/utils";

export default function PortalAuthPage() {
  const router = useRouter();
  const token = useDynamicId();

  const [status, setStatus] = useState<"loading" | "valid" | "invalid">("loading");
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    if (!token || token === "_") return;
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/v1/auth/portal-link/${token}`, {
          credentials: "include",
        });
        const data = await res.json();
        if (cancelled) return;
        if (data.valid) {
          setStatus("valid");
        } else {
          setStatus("invalid");
          setError(data.error || "リンクが無効です");
        }
      } catch {
        if (!cancelled) {
          setStatus("invalid");
          setError("サーバーに接続できません");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [token]);

  const handleConfirm = async () => {
    setConfirming(true);
    setError("");
    try {
      const res = await fetch(`/api/v1/auth/portal-link/${token}/confirm`, {
        method: "POST",
        credentials: "include",
      });
      const data = await res.json();
      if (data.success) {
        router.push(data.redirect || "/portal");
      } else {
        setError(data.error || "ログインに失敗しました");
        setConfirming(false);
      }
    } catch {
      setError("サーバーに接続できません");
      setConfirming(false);
    }
  };

  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold bg-gradient-to-r from-emerald-400 to-teal-400 bg-clip-text text-transparent">
            Sophia
          </h1>
          <p className="text-muted-foreground text-sm mt-1">パートナーポータル</p>
        </div>

        <div className="bg-background/60 border border-border rounded-xl p-8 backdrop-blur-sm shadow-2xl">
          {status === "loading" && (
            <p className="text-center text-muted-foreground text-sm">リンクを確認しています…</p>
          )}

          {status === "invalid" && (
            <div className="text-center space-y-4">
              <h2 className="text-lg font-semibold text-foreground">ログインできません</h2>
              <p className="text-muted-foreground text-sm">{error}</p>
              <Link
                href="/portal/login"
                className="inline-block text-primary hover:text-primary/80 text-sm font-medium"
              >
                新しいリンクを取得する
              </Link>
            </div>
          )}

          {status === "valid" && (
            <div className="text-center space-y-4">
              <h2 className="text-lg font-semibold text-foreground">ログイン確認</h2>
              <p className="text-muted-foreground text-sm leading-relaxed">
                下のボタンを押すとパートナーポータルにログインします。
              </p>
              {error && (
                <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
                  {error}
                </div>
              )}
              <button
                type="button"
                onClick={handleConfirm}
                disabled={confirming}
                className="w-full py-2.5 rounded-lg bg-primary text-primary-foreground font-medium text-sm hover:bg-primary/90 disabled:opacity-50 transition-colors"
              >
                {confirming ? "ログイン中…" : "ログインする"}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
