"use client";

import { useEffect, useRef } from "react";

/**
 * Esc キーで小窓を閉じる(全部の小窓で同じ動きにする)。
 * - active が true のときだけ反応する(閉じているとき・保存中は反応しない)
 * - 小窓の上に小窓を開いたときは、いちばん手前(最後に開いた)の小窓だけが閉じる
 * 以前は詳細の小窓だけが Esc で閉じ、入力・確認の小窓は閉じなかった(Esc のあとの最初のクリックが、
 * 小窓の外側を押して閉じるだけになり、押したボタンが効かないように見えた)。
 */
const stack: Array<{ current: () => void }> = [];
let listening = false;

function onKeyDown(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  const top = stack[stack.length - 1];
  if (top) top.current();
}

export function useEscapeKey(active: boolean, onEscape: () => void): void {
  const handler = useRef(onEscape);
  useEffect(() => {
    handler.current = onEscape;
  }, [onEscape]);

  useEffect(() => {
    if (!active) return;
    stack.push(handler);
    if (!listening) {
      document.addEventListener("keydown", onKeyDown);
      listening = true;
    }
    return () => {
      const i = stack.lastIndexOf(handler);
      if (i >= 0) stack.splice(i, 1);
      if (stack.length === 0 && listening) {
        document.removeEventListener("keydown", onKeyDown);
        listening = false;
      }
    };
  }, [active]);
}
