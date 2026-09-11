"use client";

import { usePathname } from "next/navigation";
import {
  Gauge,
  CalendarCheck,
  Mail,
  Database,
  ShieldCheck,
  KeyRound,
  Briefcase,
  FileText,
  FileCheck,
  Clock,
  Calculator,
  UserCircle,
  ReceiptText,
  Users,
  Globe,
  type LucideIcon,
} from "lucide-react";
import { NotificationBell } from "@/components/layout/notification-bell";
import { resolvePageMeta, type LucideIconName } from "@/lib/page-titles";

const HIDDEN_PATHS = ["/login", "/mfa", "/portal", "/upload", "/invite", "/token"];

const LUCIDE_MAP: Record<LucideIconName, LucideIcon> = {
  Gauge,
  CalendarCheck,
  Mail,
  Database,
  ShieldCheck,
  KeyRound,
  Briefcase,
  FileText,
  FileCheck,
  Clock,
  Calculator,
  UserCircle,
  ReceiptText,
  Users,
  Globe,
};

/** 全ページ共通ヘッダー。左にルート連動タイトル（アイコン・説明付き）、右に通知ベル。 */
export function AppHeader() {
  const pathname = usePathname();
  if (HIDDEN_PATHS.some((p) => pathname.startsWith(p))) return null;

  const meta = resolvePageMeta(pathname);
  const Lucide = meta?.lucideIcon ? LUCIDE_MAP[meta.lucideIcon] : null;
  const iconColor = meta?.iconColor ?? "text-sky-400";

  return (
    <header
      className={
        // 本文の p-6 と左右位置を揃える（モバイルはハンバーガー分 pl-14）
        // サブタイトル分の高さを確保
        "sticky top-0 z-30 flex min-h-14 shrink-0 items-center justify-between gap-3 " +
        "border-b border-border bg-background/95 backdrop-blur-sm " +
        "px-6 py-2.5 pl-14 lg:pl-6"
      }
    >
      <div className="min-w-0">
        {meta && (
          <>
            <h1 className="text-xl font-bold text-foreground flex items-center gap-2 truncate">
              {Lucide ? (
                <Lucide className={`w-5 h-5 shrink-0 ${iconColor}`} />
              ) : meta.emoji ? (
                <span className={`shrink-0 ${iconColor}`}>{meta.emoji}</span>
              ) : null}
              <span className="truncate">{meta.title}</span>
            </h1>
            {meta.subtitle && (
              <p className="text-xs text-muted-foreground mt-0.5 truncate">
                {meta.subtitle}
              </p>
            )}
          </>
        )}
      </div>
      <div className="shrink-0 self-start pt-0.5">
        <NotificationBell />
      </div>
    </header>
  );
}
