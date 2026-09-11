"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { useDynamicId } from "@/lib/utils";

type InviteStatus = {
  valid: boolean;
  already_used?: boolean;
  display_name?: string;
  email?: string;
  error?: string;
  message?: string;
};

export default function InvitePage() {
  const router = useRouter();
  const uuid = useDynamicId();

  const [status, setStatus] = useState<"loading" | "ready" | "invalid">("loading");
  const [info, setInfo] = useState<InviteStatus | null>(null);
  const [error, setError] = useState("");
  const [accepting, setAccepting] = useState(false);

  useEffect(() => {
    if (!uuid || uuid === "_") return;
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/v1/invite/${uuid}/status`, { credentials: "include" });
        const data: InviteStatus = await res.json();
        if (cancelled) return;
        if (data.valid) {
          setInfo(data);
          setStatus("ready");
        } else {
          setStatus("invalid");
          setError(data.error || "招待リンクが無効です");
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
  }, [uuid]);

  const handleAccept = async () => {
    setAccepting(true);
    setError("");
    try {
      const res = await fetch(`/api/v1/invite/${uuid}/accept`, {
        method: "POST",
        credentials: "include",
      });
      const data = await res.json();
      if (data.success) {
        router.push(data.redirect || "/portal/timesheet-entry");
      } else {
        setError(data.error || "受諾に失敗しました");
        setAccepting(false);
      }
    } catch {
      setError("サーバーに接続できません");
      setAccepting(false);
    }
  };

  return (
    <div className="min-h-screen bg-background flex items-center justify-center p-4">
      <div className="w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold bg-gradient-to-r from-emerald-400 to-teal-400 bg-clip-text text-transparent">
            Sophia
          </h1>
          <p className="text-muted-foreground text-sm mt-1">パートナー招待</p>
        </div>

        <div className="bg-background/60 border border-border rounded-xl p-8 backdrop-blur-sm shadow-2xl">
          {status === "loading" && (
            <p className="text-center text-muted-foreground text-sm">招待を確認しています…</p>
          )}

          {status === "invalid" && (
            <div className="text-center space-y-4">
              <h2 className="text-lg font-semibold text-foreground">招待を受けられません</h2>
              <p className="text-muted-foreground text-sm">{error}</p>
              <Link href="/portal/login" className="inline-block text-primary text-sm font-medium">
                ログインページへ
              </Link>
            </div>
          )}

          {status === "ready" && info && (
            <div className="space-y-4">
              <h2 className="text-lg font-semibold text-foreground text-center">
                {info.already_used ? "招待は使用済みです" : "招待を受諾する"}
              </h2>
              <p className="text-muted-foreground text-sm text-center leading-relaxed">
                {info.already_used
                  ? info.message || "ログインを続行できます。"
                  : `${info.display_name || "あなた"} 様への招待です。下のボタンでポータルに入れます。`}
              </p>
              {info.email && (
                <p className="text-center text-xs text-muted-foreground">{info.email}</p>
              )}
              {error && (
                <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm">
                  {error}
                </div>
              )}
              <button
                type="button"
                onClick={handleAccept}
                disabled={accepting}
                className="w-full py-2.5 rounded-lg bg-primary text-primary-foreground font-medium text-sm hover:bg-primary/90 disabled:opacity-50 transition-colors"
              >
                {accepting ? "処理中…" : info.already_used ? "ログインして続行" : "招待を受諾してログイン"}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
