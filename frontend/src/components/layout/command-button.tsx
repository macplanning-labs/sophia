"use client";

import { Command } from "lucide-react";
import { useCommandPalette } from "@/lib/CommandPaletteContext";
import { cn } from "@/lib/utils";

/**
 * コマンドパレットを開くボタン（ヘッダー用）
 * フル機能のCommandPaletteコンポーネントはapp/layout.tsxでレンダリングされる
 */
export function CommandButton() {
  const { setOpen } = useCommandPalette();

  return (
    <button
      type="button"
      aria-label="コマンド検索を開く"
      onClick={() => setOpen(true)}
      className={cn(
        "flex h-9 w-9 items-center justify-center rounded-md",
        "border border-border bg-background text-foreground",
        "hover:bg-muted transition-colors"
      )}
      title="⌘K"
    >
      <Command className="h-4 w-4" />
    </button>
  );
}
