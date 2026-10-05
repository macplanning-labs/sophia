"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { buildNavGroups } from "@/components/layout/sidebar";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useCommandPalette } from "@/lib/CommandPaletteContext";
import { cn } from "@/lib/utils";

interface SearchResult {
  label: string;
  href: string;
}

/**
 * サイドバーのナビグループからすべてのアイテムを平坦化して検索対象にする
 */
function getAllNavItems(isAdmin: boolean): SearchResult[] {
  const groups = buildNavGroups(isAdmin);
  const items: SearchResult[] = [];
  for (const group of groups) {
    for (const item of group.items) {
      items.push({ label: item.label, href: item.href });
    }
  }
  return items;
}

/**
 * 検索クエリに基づいてアイテムをフィルタリング（大文字小文字無視）
 */
function filterItems(query: string, items: SearchResult[]): SearchResult[] {
  if (!query.trim()) return items;
  const q = query.toLowerCase();
  return items.filter((item) => item.label.toLowerCase().includes(q));
}

export function CommandPalette() {
  const router = useRouter();
  const { isAdmin } = useCurrentUser();
  const { open, setOpen } = useCommandPalette();
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // ナビアイテムを取得
  const allItems = getAllNavItems(isAdmin);
  const results = filterItems(query, allItems);

  // ⌘K と Ctrl+K でモーダル開閉
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setOpen(!open);
        setQuery("");
        setSelectedIndex(0);
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, setOpen]);

  // モーダル開いたら入力にフォーカス
  useEffect(() => {
    if (open) {
      inputRef.current?.focus();
    }
  }, [open]);

  // Esc で閉じる
  useEffect(() => {
    if (!open) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, setOpen]);

  // 背景クリックで閉じる
  useEffect(() => {
    if (!open) return;

    const handler = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };

    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open, setOpen]);

  // ↑↓ キー対応
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" && results.length > 0) {
      e.preventDefault();
      const selected = results[selectedIndex];
      if (selected) {
        router.push(selected.href);
        setOpen(false);
        setQuery("");
        setSelectedIndex(0);
      }
    }
  };

  if (!open) {
    return null;
  }

  return (
    <div
      ref={rootRef}
      className="fixed inset-0 z-[70] flex items-start justify-center pt-20"
    >
      {/* 背景オーバーレイ */}
      <div
        className="absolute inset-0 bg-black/50"
        aria-hidden
      />

      {/* モーダルコンテナ */}
      <div className="relative w-full max-w-2xl mx-4 z-10">
        <div className="rounded-lg border border-border bg-background shadow-lg">
          {/* 検索入力 */}
          <div className="border-b border-border px-4 py-3">
            <input
              ref={inputRef}
              type="text"
              placeholder="ページを検索..."
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setSelectedIndex(0);
              }}
              onKeyDown={handleKeyDown}
              className={cn(
                "w-full bg-transparent text-foreground outline-none",
                "placeholder-muted-foreground"
              )}
            />
          </div>

          {/* 検索結果リスト */}
          <div className="max-h-96 overflow-auto">
            {results.length === 0 ? (
              <div className="px-4 py-6 text-center text-sm text-muted-foreground">
                該当のページが見つかりません
              </div>
            ) : (
              <ul className="p-2 space-y-1">
                {results.map((item, index) => (
                  <li key={item.href}>
                    <button
                      type="button"
                      onClick={() => {
                        router.push(item.href);
                        setOpen(false);
                        setQuery("");
                        setSelectedIndex(0);
                      }}
                      onMouseEnter={() => setSelectedIndex(index)}
                      className={cn(
                        "w-full text-left px-3 py-2 rounded-md text-sm transition-colors",
                        index === selectedIndex
                          ? "bg-primary text-primary-foreground"
                          : "hover:bg-muted"
                      )}
                    >
                      {item.label}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {/* フッター（ショートカットヒント） */}
          {results.length > 0 && (
            <div className="border-t border-border px-4 py-2 text-xs text-muted-foreground flex justify-between">
              <span>↑↓ で選択 · Enter で移動</span>
              <span>Esc で閉じる</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
