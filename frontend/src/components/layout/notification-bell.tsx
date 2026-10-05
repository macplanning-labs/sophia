"use client";

import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { Bell } from "lucide-react";
import { fetchNotifications } from "@/lib/api";
import type { NotificationItem } from "@/lib/types";
import { cn } from "@/lib/utils";
import Link from "next/link";

export function NotificationBell() {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const { data } = useQuery({
    queryKey: ["notifications"],
    queryFn: fetchNotifications,
    refetchInterval: 60 * 1000,
  });

  const items = data?.items ?? [];
  const count = data?.count ?? 0;

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const badgeLabel = count > 99 ? "99+" : String(count);

  const onItemClick = (item: NotificationItem) => {
    setOpen(false);
    router.push(item.href);
  };

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label="通知"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "relative flex h-9 w-9 items-center justify-center rounded-md",
          "border border-amber-500/40 bg-background text-amber-400",
          "hover:border-amber-400 transition-colors shadow-sm"
        )}
      >
        <Bell className="h-4 w-4" />
        {count > 0 && (
          <span
            className={cn(
              "absolute -top-1 -right-1 min-w-[15px] h-[15px] px-0.5",
              "rounded-full bg-red-500 text-white text-[9px] font-bold",
              "flex items-center justify-center leading-none"
            )}
          >
            {badgeLabel}
          </span>
        )}
      </button>

      {open && (
        <div
          className={cn(
            "absolute right-0 top-11 z-50 w-72 max-h-80 overflow-y-auto",
            "rounded-lg border border-border bg-popover text-popover-foreground",
            "shadow-lg p-2"
          )}
        >
          <div className="px-2 py-1.5 text-xs font-semibold text-foreground border-b border-border mb-1">
            要対応の期限
            {count > 0 && (
              <span className="ml-1.5 text-muted-foreground font-normal">
                {count}件
                {(data?.overdue_count ?? 0) > 0 && `（超過 ${data?.overdue_count}）`}
              </span>
            )}
          </div>

          {items.length === 0 ? (
            <p className="px-2 py-4 text-xs text-muted-foreground text-center">
              現在、要対応の期限はありません
            </p>
          ) : (
            <ul className="space-y-0.5">
              {items.map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    onClick={() => onItemClick(item)}
                    className="w-full text-left rounded-md px-2 py-2 hover:bg-muted/60 transition-colors"
                  >
                    <div className="flex items-center gap-1.5 mb-0.5">
                      <span className="text-[10px] font-medium text-muted-foreground">
                        {item.category_label}
                      </span>
                      <span
                        className={cn(
                          "text-[10px] font-semibold",
                          item.severity === "overdue" ? "text-red-400" : "text-amber-400"
                        )}
                      >
                        {item.severity === "overdue"
                          ? `${Math.abs(item.days_remaining)}日超過`
                          : `あと${item.days_remaining}日`}
                      </span>
                    </div>
                    <div className="text-xs font-medium text-foreground truncate">{item.title}</div>
                    <div className="text-[11px] text-muted-foreground truncate">{item.message}</div>
                  </button>
                </li>
              ))}
            </ul>
          )}

          <div className="border-t border-border mt-1 pt-1 px-1">
            <Link
              href="/"
              onClick={() => setOpen(false)}
              className="block text-[11px] text-sky-400 hover:underline px-1 py-1"
            >
              ダッシュボードへ →
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
