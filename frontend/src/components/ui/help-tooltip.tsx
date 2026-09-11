"use client";

import Link from "next/link";
import { useRef, useState } from "react";

interface HelpTooltipProps {
  /** ホバーした時に出す説明文 */
  message: string;
  /** ジャンプさせたい画面のURL */
  linkUrl: string;
  /** リンクボタンの文字 */
  linkLabel: string;
}

/**
 * ホバーすると説明文＋次アクションへのリンクを出すヒントアイコン(💡)。
 *
 * CSSのgroup-hoverだけだと、トリガーからバブルへマウスを移動する一瞬の
 * 隙間で当たり判定が途切れ、消えたり消えなかったりする（ブラウザのヒット
 * テストのタイミング次第でブレる）。開閉をReactのstateで管理し、離れてから
 * 少し待ってから閉じる猶予期間を設けることで、隙間を通過する間に閉じて
 * しまうのを防ぐ。
 */
export function HelpTooltip({ message, linkUrl, linkLabel }: HelpTooltipProps) {
  const [open, setOpen] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelClose = () => {
    if (closeTimer.current) {
      clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
  };

  const openNow = () => {
    cancelClose();
    setOpen(true);
  };

  const scheduleClose = () => {
    cancelClose();
    closeTimer.current = setTimeout(() => setOpen(false), 200);
  };

  return (
    <span
      className="relative inline-flex"
      onClick={(e) => e.stopPropagation()}
      onMouseEnter={openNow}
      onMouseLeave={scheduleClose}
    >
      <span className="text-xs leading-none cursor-help">💡</span>
      {open && (
        <span
          className="absolute left-1/2 -translate-x-1/2 bottom-full pb-2 z-20 w-64"
          onMouseEnter={openNow}
          onMouseLeave={scheduleClose}
        >
          <span className="block rounded-lg border border-border bg-popover text-popover-foreground text-xs p-3 shadow-lg text-left normal-case">
            <span className="block mb-2 leading-relaxed">{message}</span>
            <Link href={linkUrl} className="text-sky-400 hover:text-sky-300 underline font-medium">
              {linkLabel} →
            </Link>
          </span>
        </span>
      )}
    </span>
  );
}
