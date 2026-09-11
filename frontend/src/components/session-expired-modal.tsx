"use client";

// components/session-expired-modal.tsx — セッション期限切れ通知モーダル
//
// 401検知時に lib/api.ts の fetchJson/apiUpload/apiDownload から notifySessionExpired() 経由で
// 通知される。閉じるボタン・背景クリックでの解除はあえて提供しない
// （401後は正当なセッションが存在せず、閉じても画面操作はどのみち失敗し続けるため）。

import { useEffect, useState } from "react";
import { subscribeSessionExpired } from "@/lib/session-expired";

export function SessionExpiredModal() {
  const [open, setOpen] = useState(false);
  const [loginUrl, setLoginUrl] = useState("/login");

  useEffect(() => {
    return subscribeSessionExpired((url) => {
      setLoginUrl(url);
      setOpen(true);
    });
  }, []);

  if (!open) return null;

  const handleGoToLogin = () => {
    // usePathname/useSearchParamsではなく window から直接取る。
    // このモーダルはルートlayoutに常駐しており、useSearchParams をここで使うと
    // 静的エクスポート時に全ページがSuspense境界を要求されてしまうため。
    const currentPath = window.location.pathname + window.location.search;
    window.location.href = `${loginUrl}?redirect=${encodeURIComponent(currentPath)}`;
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-card border border-border rounded-xl shadow-2xl w-full max-w-sm mx-4 p-6 text-center space-y-4 animate-in fade-in zoom-in-95 duration-200">
        <h3 className="text-base font-semibold text-foreground">
          セッションの期限が切れました
        </h3>
        <p className="text-sm text-muted-foreground">
          再度ログインしてください。
        </p>
        <button
          onClick={handleGoToLogin}
          className="w-full py-2.5 bg-gradient-to-r from-primary to-primary/80 hover:from-blue-500 hover:to-indigo-500 text-foreground font-medium rounded-lg transition-all duration-200 shadow-lg shadow-primary/25"
        >
          ログイン画面へ
        </button>
      </div>
    </div>
  );
}
