"use client";

import { ChevronLeft, ChevronRight } from "lucide-react";
import { useCurrentMonth } from "@/lib/MonthContext";
import { cn } from "@/lib/utils";

/**
 * 月を減らす（前月に移動）
 */
function previousMonth(month: string): string {
  const [year, monthStr] = month.split("-").map(Number);
  if (monthStr === 1) {
    return `${year - 1}-12`;
  }
  return `${year}-${String(monthStr - 1).padStart(2, "0")}`;
}

/**
 * 月を増やす（翌月に移動）
 */
function nextMonth(month: string): string {
  const [year, monthStr] = month.split("-").map(Number);
  if (monthStr === 12) {
    return `${year + 1}-01`;
  }
  return `${year}-${String(monthStr + 1).padStart(2, "0")}`;
}

/**
 * "2026年10月" 形式のテキストを生成
 */
function formatMonthLabel(month: string): string {
  const [year, monthStr] = month.split("-").map(Number);
  const monthNames = [
    "1月", "2月", "3月", "4月", "5月", "6月",
    "7月", "8月", "9月", "10月", "11月", "12月"
  ];
  return `${year}年${monthNames[monthStr - 1]}`;
}

export function MonthSwitcher() {
  const { month, setMonth } = useCurrentMonth();

  return (
    <div className="flex items-center gap-2">
      {/* 前月ボタン */}
      <button
        type="button"
        aria-label="前月"
        onClick={() => setMonth(previousMonth(month))}
        className={cn(
          "flex h-9 w-9 items-center justify-center rounded-md",
          "border border-border bg-background text-foreground",
          "hover:bg-muted transition-colors"
        )}
      >
        <ChevronLeft className="h-4 w-4" />
      </button>

      {/* 月表示。文字の上に透明な月入力を重ね、クリックでカレンダーを開く(表示は1つだけ) */}
      <div className="relative">
        <div
          className={cn(
            "flex h-9 min-w-28 items-center justify-center px-3 rounded-md border border-border",
            "bg-background text-foreground text-sm font-medium hover:border-foreground transition-colors"
          )}
        >
          {formatMonthLabel(month)}
        </div>
        <input
          type="month"
          aria-label="月を選択"
          value={month}
          onChange={(e) => {
            if (e.target.value) setMonth(e.target.value);
          }}
          onClick={(e) => {
            try {
              e.currentTarget.showPicker?.();
            } catch {
              // 対応していないブラウザでは、入力欄の標準の操作に任せる
            }
          }}
          className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        />
      </div>

      {/* 翌月ボタン */}
      <button
        type="button"
        aria-label="翌月"
        onClick={() => setMonth(nextMonth(month))}
        className={cn(
          "flex h-9 w-9 items-center justify-center rounded-md",
          "border border-border bg-background text-foreground",
          "hover:bg-muted transition-colors"
        )}
      >
        <ChevronRight className="h-4 w-4" />
      </button>
    </div>
  );
}
