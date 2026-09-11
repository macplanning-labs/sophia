"use client";

import { cn } from "@/lib/utils";
import type { ListFilterMode } from "@/lib/needs-action";

const OPTIONS: { value: ListFilterMode; label: string; activeClass: string }[] = [
  { value: "pending", label: "要対応", activeClass: "text-indigo-400 bg-indigo-500/15" },
  { value: "all", label: "全て", activeClass: "text-muted-foreground bg-muted/40" },
];

interface ActionFilterChipsProps {
  value: ListFilterMode;
  onChange: (mode: ListFilterMode) => void;
  pendingCount?: number;
  totalCount?: number;
}

/** 一覧の既定フィルタ切替(要対応 / 全て)。旧PipelineSectionチップの簡易版。 */
export function ActionFilterChips({
  value,
  onChange,
  pendingCount,
  totalCount,
}: ActionFilterChipsProps) {
  return (
    <div className="flex items-center gap-2 flex-wrap mb-3">
      {OPTIONS.map(({ value: v, label, activeClass }) => (
        <button
          key={v}
          type="button"
          onClick={() => onChange(v)}
          className={cn(
            "text-xs px-2.5 py-1 rounded-md transition-opacity",
            activeClass,
            value === v ? "font-bold opacity-100" : "opacity-50 hover:opacity-75"
          )}
        >
          {label}
          {v === "pending" && pendingCount != null && (
            <span className="ml-1 tabular-nums">({pendingCount})</span>
          )}
          {v === "all" && totalCount != null && (
            <span className="ml-1 tabular-nums">({totalCount})</span>
          )}
        </button>
      ))}
    </div>
  );
}
