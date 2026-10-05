"use client";

import { useRef } from "react";

type Props = Omit<React.InputHTMLAttributes<HTMLInputElement>, "type">;

/** 日付入力の共通部品。ブラウザ標準のカレンダーアイコンはダークテーマで見えないため、
 *  標準アイコンを隠し、📅 ボタンでカレンダーを開く。 */
export function DateInput({ className, disabled, ...props }: Props) {
  const ref = useRef<HTMLInputElement>(null);
  const open = () => {
    const el = ref.current;
    if (!el || disabled) return;
    try {
      el.showPicker();
    } catch {
      el.focus();
    }
  };
  return (
    <span className="relative block w-full">
      <input
        ref={ref}
        type="date"
        disabled={disabled}
        className={`w-full pr-9 [&::-webkit-calendar-picker-indicator]:hidden ${className ?? ""}`}
        {...props}
      />
      <button
        type="button"
        tabIndex={-1}
        onClick={open}
        disabled={disabled}
        aria-label="カレンダーを開く"
        className="absolute right-2 top-1/2 -translate-y-1/2 text-base leading-none disabled:opacity-50"
      >
        📅
      </button>
    </span>
  );
}
