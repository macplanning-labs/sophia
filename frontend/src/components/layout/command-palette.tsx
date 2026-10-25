"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useQuery } from "@tanstack/react-query";
import { fetchMembers, fetchMastersData, searchDocuments } from "@/lib/api";
import { useDebounced } from "@/lib/useDebounced";
import { buildNavGroups } from "@/components/layout/sidebar";
import { visibleTabs } from "@/lib/section-tabs";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useCommandPalette } from "@/lib/CommandPaletteContext";
import { cn } from "@/lib/utils";

interface SearchResult {
  label: string;
  href: string;
  /** 2行目(取引先・月など) */
  sub?: string;
  /** 右端のタグ(帳票の種類・状況、要員の区分など) */
  tag?: string;
}

interface ResultGroup {
  title: string;
  items: SearchResult[];
}

const DOC_TYPE_LABEL: Record<string, string> = {
  received_order: "受注",
  order: "注文書",
  invoice: "請求書",
  notice: "支払通知",
};

const MAX_DOCS = 20;
const MAX_OTHERS = 5;

interface PartyRow {
  id?: number;
  partner_id?: string;
  name: string;
}

/**
 * サイドバーの項目を平坦化して検索対象にする。
 * タブでまとめた項目(勤務表・月の締めなど)は、ボタンではなく各タブの画面を出す(サブにセクション名)
 */
export function getAllNavItems(isAdmin: boolean): SearchResult[] {
  const groups = buildNavGroups(isAdmin);
  const items: SearchResult[] = [];
  for (const group of groups) {
    for (const item of group.items) {
      if (item.adminOnly && !isAdmin) continue;
      if (item.section) {
        for (const tab of visibleTabs(item.section, isAdmin)) {
          items.push({ label: tab.label, href: tab.href, sub: item.section.label });
        }
      } else {
        items.push({ label: item.label, href: item.href });
      }
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
  const pages = filterItems(query, allItems);

  // 帳票・要員・取引先の横断検索(管理者のみ)。帳票は入力が0.2秒止まってから検索する
  const q = query.trim();
  const dq = useDebounced(q, 200);
  const searching = open && isAdmin && q.length > 0;
  const { data: docs = [] } = useQuery({
    queryKey: ["palette-docs", dq],
    queryFn: () => searchDocuments(dq, MAX_DOCS),
    enabled: open && isAdmin && dq.length > 0,
    staleTime: 30_000,
  });
  const { data: members = [] } = useQuery({ queryKey: ["members"], queryFn: fetchMembers, enabled: searching });
  const { data: clients = [] } = useQuery<PartyRow[]>({
    queryKey: ["masters-data", "clients"], queryFn: () => fetchMastersData("clients"), enabled: searching,
  });
  const { data: partners = [] } = useQuery<PartyRow[]>({
    queryKey: ["masters-data", "partners"], queryFn: () => fetchMastersData("partners"), enabled: searching,
  });

  const groups: ResultGroup[] = [];
  if (pages.length > 0) groups.push({ title: "ページ", items: pages });
  if (searching) {
    const lq = q.toLowerCase();
    const docItems = (q === dq ? docs : []).map((d) => ({
      label: `${d.number}${d.title ? `  ${d.title}` : ""}`,
      sub: [d.counterparty, d.month ? d.month.replace("-", "年") + "月" : ""].filter(Boolean).join(" ・ "),
      tag: `${DOC_TYPE_LABEL[d.type] ?? d.type} / ${d.status}`,
      href: d.url,
    }));
    if (docItems.length > 0) groups.push({ title: "帳票", items: docItems });
    const memberItems = members
      .filter((m) => m.is_active && (m.name.toLowerCase().includes(lq) || m.name_kana.toLowerCase().includes(lq)))
      .slice(0, MAX_OTHERS)
      .map((m) => ({
        label: m.name,
        tag: m.affiliation_type === "EMPLOYEE" ? "自社社員" : "パートナー要員",
        href: `/members/${m.id}`,
      }));
    if (memberItems.length > 0) groups.push({ title: "要員", items: memberItems });
    const partyItems = [
      ...clients.filter((c) => c.name?.toLowerCase().includes(lq)).map((c) => ({ label: c.name, tag: "クライアント", href: `/parties?type=client&open=${c.id}` })),
      ...partners.filter((c) => c.name?.toLowerCase().includes(lq)).map((c) => ({ label: c.name, tag: "パートナー", href: `/parties?type=partner&open=${c.partner_id}` })),
    ].slice(0, MAX_OTHERS);
    if (partyItems.length > 0) groups.push({ title: "取引先", items: partyItems });
  }
  const results = groups.flatMap((g) => g.items);

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
              placeholder={isAdmin ? "ページ・帳票・要員・取引先を検索..." : "ページを検索..."}
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
                該当のものが見つかりません
              </div>
            ) : (
              <div className="p-2">
                {(() => {
                  let index = -1;
                  return groups.map((g) => (
                    <div key={g.title}>
                      {searching && <div className="px-3 pt-2 pb-1 text-[11px] text-muted-foreground">{g.title}</div>}
                      <ul className="space-y-1">
                        {g.items.map((item) => {
                          index += 1;
                          const i = index;
                          return (
                            <li key={`${g.title}:${item.href}:${item.label}`}>
                              <button
                                type="button"
                                onClick={() => {
                                  router.push(item.href);
                                  setOpen(false);
                                  setQuery("");
                                  setSelectedIndex(0);
                                }}
                                onMouseEnter={() => setSelectedIndex(i)}
                                className={cn(
                                  "w-full text-left px-3 py-2 rounded-md text-sm transition-colors flex items-center justify-between gap-3",
                                  i === selectedIndex ? "bg-primary text-primary-foreground" : "hover:bg-muted"
                                )}
                              >
                                <span className="min-w-0">
                                  <span className="block truncate">{item.label}</span>
                                  {item.sub && <span className="block truncate text-xs opacity-70">{item.sub}</span>}
                                </span>
                                {item.tag && <span className="shrink-0 text-[11px] border border-current/30 rounded px-1.5 opacity-80">{item.tag}</span>}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    </div>
                  ));
                })()}
              </div>
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
