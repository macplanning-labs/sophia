"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import {
  Gauge, CalendarCheck, FileText, FileCheck,
  Clock, CalendarClock,
  Calculator, UserCircle, ReceiptText, Users, ShieldCheck, KeyRound,
  Database, Moon, Sun, LogOut, Mail, Menu, X, Briefcase, Globe,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useCurrentUser } from "@/lib/useCurrentUser";

interface NavItem {
  label: string;
  href: string;
  icon: React.ElementType;
  /** true の場合、Admin ロールのユーザーにのみ表示する */
  adminOnly?: boolean;
}

interface NavGroup {
  title: string;
  items: NavItem[];
}

function buildNavGroups(isAdmin: boolean): NavGroup[] {
  return [
    {
      title: "",
      items: [
        { label: "ダッシュボード", href: "/", icon: Gauge },
        { label: "案件", href: "/projects", icon: Briefcase },
        { label: "月次確定", href: "/settlement", icon: CalendarCheck },
      ],
    },
    {
      title: "契約管理",
      items: [
        { label: "発注契約", href: "/partner-contracts", icon: FileText },
        { label: "受注契約", href: "/client-contracts", icon: FileCheck },
      ],
    },
    {
      title: "稼働・タスク",
      items: [
        { label: "稼働報告", href: "/timesheets", icon: Clock },
        { label: "稼働報告（自己申告）", href: "/my-timesheet", icon: CalendarClock },
        // { label: "タスク", href: "/tasks", icon: CheckSquare }, // レビュー#6: 月次確定と重複のため非表示
        { label: "受信メール", href: "/received-emails", icon: Mail, adminOnly: true },
        { label: "Peppolログ", href: "/peppol", icon: Globe, adminOnly: true },
      ],
    },
    {
      title: "給与",
      items: [
        // メニュー文言はロールに関わらず「給与」で統一。
        // 表示先の /payroll は同じだが、バックエンド側(api_index)で
        // Employeeロールの場合は自分自身のemployee_idの行のみが返るようフィルタ済みなので、
        // 一般社員が開いても実際に見えるのは自分の給与データのみ。
        { label: "給与", href: "/payroll", icon: Calculator },
        { label: "社員", href: "/employees", icon: UserCircle },
        { label: "経費", href: "/expenses", icon: ReceiptText },
      ],
    },
    {
      title: "設定",
      items: [
        { label: "ユーザー管理", href: "/users", icon: Users, adminOnly: true },
        { label: "APIキー", href: "/settings/api-keys", icon: KeyRound, adminOnly: true },
        { label: "セキュリティ", href: "/settings/security", icon: ShieldCheck },
        { label: "マスタメンテ", href: "/masters", icon: Database, adminOnly: true },
      ],
    },
  ];
}

export function Sidebar() {
  const pathname = usePathname();
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [mobileOpen, setMobileOpen] = useState(false);
  const [desktopCollapsed, setDesktopCollapsed] = useState(false);
  const { isAdmin } = useCurrentUser();
  const navGroups = buildNavGroups(isAdmin);

  useEffect(() => {
    const saved = localStorage.getItem("sophia-theme") as "dark" | "light" | null;
    if (saved) setTheme(saved);
    const collapsed = localStorage.getItem("sophia-sidebar-collapsed");
    if (collapsed === "1") setDesktopCollapsed(true);
  }, []);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    localStorage.setItem("sophia-theme", theme);
  }, [theme]);

  useEffect(() => {
    localStorage.setItem("sophia-sidebar-collapsed", desktopCollapsed ? "1" : "0");
  }, [desktopCollapsed]);

  // ページ遷移時にモバイルドロワーを閉じる
  useEffect(() => {
    setMobileOpen(false);
  }, [pathname]);

  // ログイン・パートナーポータル・スマホアップロードページ（未認証アクセス）ではSidebar非表示
  // ※ フックの後に配置（Reactのルール: フックは条件分岐の前に呼ぶ）
  const hiddenPaths = ["/login", "/mfa", "/portal", "/upload", "/invite", "/token"];
  if (hiddenPaths.some(p => pathname.startsWith(p))) return null;

  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);

  return (
    <>
      {/* モバイル用ハンバーガーボタン（lg以上では非表示） */}
      <button
        onClick={() => setMobileOpen(true)}
        aria-label="メニューを開く"
        className="lg:hidden fixed top-3 left-3 z-50 p-2 rounded-md bg-sidebar text-sidebar-foreground border border-sidebar-border shadow-md"
      >
        <Menu className="w-5 h-5" />
      </button>

      {/* デスクトップ用: 折りたたみ時の再展開ボタン */}
      {desktopCollapsed && (
        <button
          onClick={() => setDesktopCollapsed(false)}
          aria-label="メニューを開く"
          className="hidden lg:flex fixed top-3 left-3 z-50 p-2 rounded-md bg-sidebar text-sidebar-foreground border border-sidebar-border shadow-md"
        >
          <Menu className="w-5 h-5" />
        </button>
      )}

      {/* モバイル用オーバーレイ */}
      {mobileOpen && (
        <div
          onClick={() => setMobileOpen(false)}
          className="lg:hidden fixed inset-0 z-40 bg-black/50"
        />
      )}

      <aside
        className={cn(
          "flex flex-col w-60 h-screen shrink-0 bg-sidebar text-sidebar-foreground border-r border-sidebar-border overflow-y-auto",
          "fixed inset-y-0 left-0 z-50 transition-transform duration-200",
          mobileOpen ? "translate-x-0" : "-translate-x-full",
          "lg:translate-x-0 lg:sticky lg:top-0 lg:z-auto lg:self-start lg:max-h-screen",
          desktopCollapsed && "lg:hidden"
        )}
      >
        {/* ロゴ */}
        <div className="px-4 py-4 border-b border-border flex items-center justify-between">
          <div>
            <h1 className="text-lg font-bold flex items-center gap-2">
              <span className="inline-block w-5 h-5 bg-primary rounded-md" />
              Sophia
            </h1>
            <p className="text-xs text-muted-foreground mt-0.5">SES受発注管理</p>
          </div>
          <button
            onClick={() => { setMobileOpen(false); setDesktopCollapsed(true); }}
            aria-label="メニューを閉じる"
            className="p-1 text-muted-foreground hover:text-foreground"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

      {/* ナビゲーション */}
      <nav className="flex-1 py-2 px-2 space-y-1">
        {navGroups.map((group, gi) => (
          <div key={gi}>
            {group.title && (
              <p className="text-[11px] text-muted-foreground uppercase tracking-wider px-3 pt-4 pb-1 font-medium">
                {group.title}
              </p>
            )}
            {group.items.filter((item) => !item.adminOnly || isAdmin).map((item) => {
              const Icon = item.icon;
              const active = isActive(item.href);
              return (
                <Link
                  key={item.href}
                  href={item.href}
                  className={cn(
                    "flex items-center gap-2.5 px-3 py-2 rounded-md text-sm transition-all duration-150",
                    active
                      ? "bg-sidebar-primary/10 text-sidebar-primary font-semibold border-r-[3px] border-sidebar-primary"
                      : "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent"
                  )}
                >
                  <Icon className="w-4 h-4 shrink-0" />
                  {item.label}
                </Link>
              );
            })}
          </div>
        ))}
      </nav>

      {/* テーマ切替 & ログアウト */}
      <div className="p-2 border-t border-border space-y-1">
        <button
          onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm text-muted-foreground hover:text-foreground hover:bg-sidebar-accent transition-colors"
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
        <button
          onClick={async () => {
            await fetch("/api/v1/auth/logout", { method: "POST", credentials: "include" });
            window.location.href = "/login";
          }}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm text-red-400 hover:text-red-300 hover:bg-red-500/10 transition-colors"
        >
          <LogOut className="w-4 h-4" />
          ログアウト
        </button>
      </div>
      </aside>
    </>
  );
}
