"use client";

import { useEffect, useRef, useState } from "react";
import { Moon, Sun, LogOut, Check, User } from "lucide-react";
import { avatarLabel } from "@/lib/avatar";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useUiV2 } from "@/lib/useUiV2";
import { visibleNavV2 } from "@/lib/nav-v2";
import { roleLabel } from "@/components/layout/sidebar";
import { handleLogout, toggleTheme } from "@/lib/auth-actions";
import { cn } from "@/lib/utils";

export function AccountMenu() {
  const { isAdmin, user } = useCurrentUser();
  const { enabled: uiV2Enabled, setEnabled: setUiV2Enabled } = useUiV2();
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // テーマを初期化
  useEffect(() => {
    const saved = localStorage.getItem("sophia-theme") as "dark" | "light" | null;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    if (saved) setTheme(saved);
  }, []);

  // テーマ変更時に localStorage に保存
  const handleThemeToggle = () => {
    const newTheme = toggleTheme(theme);
    setTheme(newTheme);
    document.documentElement.classList.toggle("dark", newTheme === "dark");
    localStorage.setItem("sophia-theme", newTheme);
  };

  // 背景クリックで閉じる
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  // 新UI無効または表示対象アイテムなしの場合は何も描画しない
  const visibleV2Items = visibleNavV2(isAdmin);
  if (!uiV2Enabled || visibleV2Items.length === 0) {
    return null;
  }

  const roleInfo = roleLabel(user?.role);
  const avatar = avatarLabel(user?.display_name);

  return (
    <div ref={rootRef} className="relative">
      {/* ユーザーボタン */}
      <button
        type="button"
        aria-label="アカウントメニュー"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "flex h-9 w-9 items-center justify-center rounded-md",
          "border border-border bg-background text-foreground text-sm font-medium",
          "hover:bg-muted transition-colors"
        )}
      >
        {avatar ?? <User className="h-4 w-4" aria-hidden />}
      </button>

      {/* ドロップダウンメニュー */}
      {open && (
        <div
          className={cn(
            "absolute top-full right-0 mt-2 w-56 z-50",
            "rounded-lg border border-border bg-background",
            "shadow-lg divide-y divide-border"
          )}
        >
          {/* ユーザー情報セクション */}
          <div className="px-4 py-3">
            <p className="text-xs text-muted-foreground">ログイン中</p>
            {user?.display_name && (
              <p className="text-sm font-semibold text-foreground truncate">{user.display_name}</p>
            )}
            <p
              className={cn(
                "truncate",
                user?.display_name ? "text-xs text-muted-foreground" : "text-sm font-medium text-foreground"
              )}
              title={user?.email}
            >
              {user?.email ?? "—"}
            </p>
            <div className="mt-2">
              <span
                className={cn(
                  "inline-block px-1.5 py-0.5 rounded border text-[11px]",
                  isAdmin
                    ? "border-emerald-600/50 text-emerald-400"
                    : "border-border text-muted-foreground"
                )}
              >
                権限: {roleInfo.label}
              </span>
            </div>
          </div>

          {/* アクション */}
          <div className="py-1">
            {/* テーマ切り替え */}
            <button
              onClick={() => {
                handleThemeToggle();
                setOpen(false);
              }}
              className={cn(
                "flex items-center gap-2 w-full px-4 py-2 text-sm",
                "text-muted-foreground hover:text-foreground hover:bg-muted",
                "transition-colors"
              )}
            >
              {theme === "dark" ? (
                <>
                  <Moon className="w-4 h-4" />
                  ダークモード
                </>
              ) : (
                <>
                  <Sun className="w-4 h-4" />
                  ライトモード
                </>
              )}
            </button>

            {/* 新UI切り替え */}
            <button
              onClick={() => {
                setUiV2Enabled(!uiV2Enabled);
                setOpen(false);
              }}
              className={cn(
                "flex items-center gap-2 w-full px-4 py-2 text-sm",
                "text-muted-foreground hover:text-foreground hover:bg-muted",
                "transition-colors"
              )}
            >
              <span className="inline-flex items-center w-4 h-4">
                {uiV2Enabled ? <Check className="w-4 h-4" /> : " "}
              </span>
              新UI
            </button>

            {/* ログアウト */}
            <button
              onClick={() => {
                setOpen(false);
                handleLogout();
              }}
              className={cn(
                "flex items-center gap-2 w-full px-4 py-2 text-sm",
                "text-red-400 hover:text-red-300 hover:bg-red-500/10",
                "transition-colors"
              )}
            >
              <LogOut className="w-4 h-4" />
              ログアウト
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
