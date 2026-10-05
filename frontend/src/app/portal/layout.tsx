"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { cn } from "@/lib/utils";
import { ClipboardList, FileText, FileSpreadsheet, BookOpen, LogOut } from "lucide-react";
import { toast } from "sonner";

const navItems = [
  { label: "注文一覧", href: "/portal", icon: ClipboardList },
  { label: "請求一覧", href: "/portal/notices", icon: FileText },
  { label: "稼働報告", href: "/portal/timesheets", icon: FileSpreadsheet },
  { label: "マニュアル", href: "/portal/manual", icon: BookOpen },
];

export default function PortalLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();
  const router = useRouter();

  // ログイン・マジックリンク確認はポータル chrome なし
  if (pathname.startsWith("/portal/login") || pathname.startsWith("/portal/auth")) {
    return <>{children}</>;
  }

  const handleLogout = async () => {
    try {
      await fetch("/api/v1/auth/logout", { method: "POST", credentials: "include" });
      toast.success("ログアウトしました");
      router.push("/portal/login");
    } catch {
      window.location.href = "/portal/login";
    }
  };

  const isActive = (href: string) => {
    if (href === "/portal") return pathname === "/portal";
    return pathname.startsWith(href);
  };

  return (
    <div className="flex min-h-screen">
      {/* サイドバー */}
      <aside className="w-56 shrink-0 border-r border-border bg-card/50 flex flex-col">
        {/* ロゴ */}
        <div className="h-14 flex items-center gap-2 px-4 border-b border-border">
          <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-blue-500 to-cyan-400 flex items-center justify-center text-white font-bold text-sm">
            S
          </div>
          <span className="text-sm font-semibold text-foreground tracking-wide">Sophia</span>
          <span className="text-[10px] text-muted-foreground ml-auto">Partner</span>
        </div>

        {/* ナビゲーション */}
        <nav className="flex-1 py-3 px-2 space-y-0.5">
          {navItems.map((item) => {
            const Icon = item.icon;
            const active = isActive(item.href);
            return (
              <Link
                key={item.href}
                href={item.href}
                className={cn(
                  "flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-all duration-150",
                  active
                    ? "bg-primary/10 text-primary font-medium shadow-sm"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
                )}
              >
                <Icon className={cn("w-4 h-4", active ? "text-primary" : "text-muted-foreground")} />
                {item.label}
              </Link>
            );
          })}
        </nav>

        {/* ログアウト */}
        <div className="p-2 border-t border-border">
          <button
            onClick={handleLogout}
            className="flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm text-muted-foreground hover:bg-red-500/10 hover:text-red-400 transition-colors w-full"
          >
            <LogOut className="w-4 h-4" />
            ログアウト
          </button>
        </div>
      </aside>

      {/* メインコンテンツ */}
      <main className="flex-1 overflow-auto">{children}</main>
    </div>
  );
}
