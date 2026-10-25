"use client";

/**
 * サイドバーのセクション(ホーム・契約・勤務表・月の締め・設定など)の画面の上部のタブの帯(DEMO-000184・000185・000188)。
 * ルートのレイアウトで、ヘッダーとページの間に1か所だけ置く。セクションに属する画面でだけ出る。
 */

import Link from "next/link";
import { usePathname } from "next/navigation";
import { cn } from "@/lib/utils";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { isChromeHidden } from "@/lib/layout-paths";
import { activeTab, sectionOfPath, visibleTabs } from "@/lib/section-tabs";

export function SectionTabs() {
  const pathname = usePathname();
  const { isAdmin } = useCurrentUser();

  if (!pathname || isChromeHidden(pathname)) return null;
  const section = sectionOfPath(pathname);
  if (!section) return null;

  const tabs = visibleTabs(section, isAdmin);
  const active = activeTab(section, pathname);

  return (
    <nav
      aria-label={section.label}
      data-testid="section-tabs"
      className="shrink-0 border-b border-border bg-background px-4 lg:px-8"
    >
      <ul role="tablist" className="flex gap-1 overflow-x-auto pb-px -mb-px">
        {tabs.map((tab) => {
          const selected = tab.key === active?.key;
          return (
            <li key={tab.key} role="presentation" className="shrink-0">
              <Link
                href={tab.href}
                role="tab"
                aria-selected={selected}
                aria-current={selected ? "page" : undefined}
                className={cn(
                  "block whitespace-nowrap border-b-2 px-4 py-3 text-sm transition-colors -mb-px",
                  selected
                    ? "border-sidebar-primary font-semibold text-sidebar-primary"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                )}
              >
                {tab.label}
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
