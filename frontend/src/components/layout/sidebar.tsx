"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import {
  Home, Mail, Briefcase, FileText, Building2, Clock, CalendarCheck, Calculator, Settings,
  Moon, Sun, LogOut, Menu, X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useCurrentUser } from "@/lib/useCurrentUser";
import type { CurrentUser } from "@/lib/api";
import { isChromeHidden } from "@/lib/layout-paths";
import { handleLogout } from "@/lib/auth-actions";
import {
  CLOSING_SECTION,
  CONTRACTS_SECTION,
  HOME_SECTION,
  MAIL_SECTION,
  PARTIES_SECTION,
  SETTINGS_SECTION,
  STAFF_SECTION,
  TIMESHEETS_SECTION,
  isInSection,
  landingHref,
  visibleTabs,
  type Section,
} from "@/lib/section-tabs";

interface NavItem {
  label: string;
  href: string;
  icon: React.ElementType;
  /** true の場合、Admin ロールのユーザーにのみ表示する */
  adminOnly?: boolean;
  /** 選択中の判定を、href の前方一致ではなく、この関数で行う(セクション: タブ配下のすべての画面で選択中にする) */
  activeWhen?: (pathname: string) => boolean;
  /** セクション(上部のタブで画面を切り替える)のボタンのとき、そのセクション(⌘K の検索で各タブの画面を出すのに使う) */
  section?: Section;
}

interface NavGroup {
  title: string;
  items: NavItem[];
}

/**
 * セクション(上部のタブで画面を切り替える)のボタン。行き先は、そのユーザーに見える最初のタブ。
 * 見えるタブが1つも無ければ、ボタンを出さない(null)。
 */
function sectionItem(section: Section, icon: React.ElementType, isAdmin: boolean): NavItem | null {
  if (visibleTabs(section, isAdmin).length === 0) return null;
  return {
    label: section.label,
    href: landingHref(section, isAdmin),
    icon,
    activeWhen: (pathname) => isInSection(section, pathname),
    section,
  };
}

/**
 * サイドバーの構成(DEMO-000188、2026-10-09 佐藤さん了承)。
 * 似た画面は1つのボタンにまとめ、押すと上部のタブ(SectionTabs)で切り替える。URLは従来のまま。
 * 新UIと旧メニューを並べていたのをやめ、1つのメニューにした(新UIの切り替えは廃止)。
 */
export function buildNavGroups(isAdmin: boolean): NavGroup[] {
  const groups: NavGroup[] = [
    {
      title: "",
      items: [
        sectionItem(HOME_SECTION, Home, isAdmin),
        sectionItem(MAIL_SECTION, Mail, isAdmin),
      ].filter((i): i is NavItem => i !== null),
    },
    {
      title: "契約",
      items: [
        { label: "案件", href: "/projects", icon: Briefcase },
        sectionItem(CONTRACTS_SECTION, FileText, isAdmin),
        sectionItem(PARTIES_SECTION, Building2, isAdmin),
      ].filter((i): i is NavItem => i !== null),
    },
    {
      title: "月次",
      items: [
        sectionItem(TIMESHEETS_SECTION, Clock, isAdmin),
        sectionItem(CLOSING_SECTION, CalendarCheck, isAdmin),
      ].filter((i): i is NavItem => i !== null),
    },
    {
      // 給与・経費は、一般社員が開いてもサーバ側で本人の分だけが返る
      title: "社員",
      items: [sectionItem(STAFF_SECTION, Calculator, isAdmin)].filter((i): i is NavItem => i !== null),
    },
    {
      // ボタンと同じ名前の見出しは付けない(DEMO-000184)
      title: "",
      items: [sectionItem(SETTINGS_SECTION, Settings, isAdmin)].filter((i): i is NavItem => i !== null),
    },
  ];
  return groups.filter((g) => g.items.some((i) => !i.adminOnly || isAdmin));
}

/** ログイン中アカウントの権限表示（サイドバー下部）。管理者専用ボタンが出ない理由を本人が確認できるようにする */
export function roleLabel(role: CurrentUser["role"] | undefined): { label: string; note?: string } {
  switch (role) {
    case "ADMIN":
      return { label: "管理者" };
    case "EMPLOYEE":
      return { label: "担当者", note: "承認・送信・編集などの管理者専用操作は表示されません" };
    case undefined:
      return { label: "確認中" };
    default:
      return { label: role };
  }
}

export function Sidebar() {
  const pathname = usePathname();
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [mobileOpen, setMobileOpen] = useState(false);
  const [desktopCollapsed, setDesktopCollapsed] = useState(false);
  const { isAdmin, user, isLoading: userLoading, isError: userError } = useCurrentUser();
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
  if (isChromeHidden(pathname)) return null;

  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);
  const isItemActive = (item: NavItem) => (item.activeWhen ? item.activeWhen(pathname) : isActive(item.href));

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
              const active = isItemActive(item);
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
        {/* ログイン中のアカウントと権限 */}
        {(() => {
          const r = userError ? { label: "取得できません", note: "再読み込みしてください" } : roleLabel(userLoading ? undefined : user?.role);
          return (
            <div className="px-3 py-2 text-xs" data-testid="current-account">
              <p className="text-muted-foreground">ログイン中</p>
              <p className="text-foreground truncate" title={user?.email}>{user?.email ?? "—"}</p>
              <p className="mt-1">
                <span
                  className={cn(
                    "inline-block px-1.5 py-0.5 rounded border text-[11px]",
                    isAdmin
                      ? "border-emerald-600/50 text-emerald-400"
                      : "border-border text-muted-foreground"
                  )}
                >
                  権限: {r.label}
                </span>
              </p>
              {r.note && <p className="mt-1 text-[11px] text-muted-foreground leading-snug">{r.note}</p>}
            </div>
          );
        })()}
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
          onClick={handleLogout}
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
