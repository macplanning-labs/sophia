"use client";

import { useEffect, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

export interface SearchableOption {
  value: string;
  label: string;
  /** 検索対象の追加テキスト（任意） */
  searchText?: string;
  /** 選択不可・薄い表示（任意） */
  muted?: boolean;
}

interface Props {
  label: string;
  options: SearchableOption[];
  value: string;
  onChange: (value: string) => void;
  /** クリア時の選択肢ラベル。統一して「すべて」 */
  allLabel?: string;
  /** false のときクリア選択肢を出さない（必須選択の列用） */
  allowClear?: boolean;
  placeholder?: string;
  className?: string;
  /** th 内で使う場合のボタン見た目 */
  compact?: boolean;
}

/** テーブル列見出し用の検索可能セレクト（ダッシュボードのプロジェクト名と同型） */
export function SearchableColumnHeader({
  label,
  options,
  value,
  onChange,
  allLabel = "すべて",
  allowClear = true,
  placeholder = "検索…",
  className,
  compact = true,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [pos, setPos] = useState({ top: 0, left: 0 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      const target = e.target as Node;
      if (popRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  const openPicker = () => {
    const r = triggerRef.current?.getBoundingClientRect();
    if (r) {
      const left = Math.min(r.left, window.innerWidth - 316);
      setPos({ top: r.bottom + 6, left: Math.max(8, left) });
    }
    setQuery("");
    setOpen(true);
  };

  const selected = options.find((o) => o.value === value);
  // value="" の「すべて」も options にあればそのラベルを表示する
  const display = selected?.label ?? (!value && allowClear ? allLabel : label);

  const filtered = options.filter((o) => {
    if (!query) return true;
    const hay = `${o.label} ${o.searchText ?? ""}`.toLowerCase();
    return hay.includes(query.toLowerCase());
  });

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          open ? setOpen(false) : openPicker();
        }}
        className={cn(
          "inline-flex items-center gap-1 hover:text-foreground transition-colors",
          compact ? "text-xs font-medium text-muted-foreground" : "text-sm",
          value && "text-primary",
          className
        )}
      >
        <span className="truncate max-w-[10rem]">{display}</span>
        <ChevronDown className={cn("w-3 h-3 shrink-0 transition-transform", open && "rotate-180")} />
      </button>

      {open && (
        <div
          ref={popRef}
          style={{ position: "fixed", top: pos.top, left: pos.left }}
          className="z-50 w-[300px] rounded-md border border-border bg-popover p-2 shadow-lg"
          onClick={(e) => e.stopPropagation()}
        >
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={placeholder}
            className="w-full mb-1.5 rounded border border-border bg-muted px-2 py-1.5 text-xs text-foreground"
          />
          <div className="max-h-64 overflow-y-auto space-y-0.5">
            {allowClear && (
              <button
                type="button"
                onClick={() => {
                  onChange("");
                  setOpen(false);
                }}
                className={cn(
                  "w-full rounded px-2 py-1.5 text-left text-xs hover:bg-muted",
                  !value && "bg-muted/60 text-primary"
                )}
              >
                {allLabel}
              </button>
            )}
            {filtered.length === 0 ? (
              <p className="py-3 text-center text-xs text-muted-foreground">該当なし</p>
            ) : (
              filtered.map((o) => (
                <button
                  type="button"
                  key={o.value}
                  onClick={() => {
                    onChange(o.value);
                    setOpen(false);
                  }}
                  className={cn(
                    "w-full flex items-center justify-between gap-2 rounded px-2 py-1.5 text-left text-xs hover:bg-muted",
                    o.value === value && "bg-muted/60 text-primary",
                    o.muted && "text-muted-foreground"
                  )}
                >
                  <span className="truncate">{o.label}</span>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </>
  );
}
