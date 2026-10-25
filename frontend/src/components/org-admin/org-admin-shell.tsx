"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { AccessDenied } from "@/components/access-denied";
import { buttonVariants } from "@/components/ui/button";

/** 管理者以外には画面を出さない（users/page.tsx と同じ判定）。 */
export function OrgAdminGate({ children }: { children: ReactNode }) {
  const { isAdmin, isLoading } = useCurrentUser();
  if (isLoading) return null;
  if (!isAdmin) return <AccessDenied message="組織・ロールの管理を利用するには管理者権限が必要です。" />;
  return <>{children}</>;
}

const TABS = [
  { key: "org", href: "/settings/org", label: "組織" },
  { key: "roles", href: "/settings/org/roles", label: "ロールと権限" },
  { key: "assignments", href: "/settings/org/assignments", label: "割り当て" },
] as const;

/** 組織・ロール・割り当ての3画面を行き来する見出しタブ。 */
export function OrgAdminTabs({ active }: { active: (typeof TABS)[number]["key"] }) {
  return (
    <div className="flex flex-wrap gap-2">
      {TABS.map((t) => (
        <Link
          key={t.key}
          href={t.href}
          aria-current={active === t.key ? "page" : undefined}
          className={buttonVariants({
            variant: active === t.key ? "default" : "outline",
            size: "sm",
            className: active === t.key ? "" : "border-border text-muted-foreground hover:text-foreground",
          })}
        >
          {t.label}
        </Link>
      ))}
    </div>
  );
}

/** エラーはトーストで、サーバーの日本語メッセージをそのまま出す。 */
export function errorMessage(e: unknown, fallback: string): string {
  return e instanceof Error && e.message ? e.message : fallback;
}
