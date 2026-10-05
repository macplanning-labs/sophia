"use client";

import React, { createContext, useContext, useEffect, useRef, useState } from "react";
import { useMounted } from "@/lib/useMounted";

interface MonthContextType {
  month: string; // "YYYY-MM" format
  setMonth: (month: string) => void;
}

const MonthContext = createContext<MonthContextType | undefined>(undefined);

/**
 * 月の妥当性チェック（YYYY-MM 形式）
 */
function isValidMonth(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = value.match(/^\d{4}-\d{2}$/);
  if (!match) return false;
  const [, month] = value.split("-").map(Number);
  return month >= 1 && month <= 12;
}

/**
 * 当月を "YYYY-MM" 形式で取得
 */
function getCurrentMonth(): string {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  return `${year}-${month}`;
}

/**
 * localStorage から月を読み取る（例外時は握りつぶす）
 */
function getStoredMonth(): string | null {
  try {
    const value = sessionStorage.getItem("sophia-month");
    if (isValidMonth(value)) {
      return value;
    }
  } catch {
    // 握りつぶす
  }
  return null;
}

/**
 * localStorage に月を保存（例外時は握りつぶす）
 */
function storeMonth(month: string): void {
  if (!isValidMonth(month)) return;
  try {
    sessionStorage.setItem("sophia-month", month);
  } catch {
    // 握りつぶす
  }
}

export function MonthProvider({ children }: { children: React.ReactNode }) {
  // 初期値は当月。SSR/hydration mismatch を防ぐため、マウント前は当月で固定。
  const [month, setMonthState] = useState<string>(getCurrentMonth());
  const mounted = useMounted();
  // 初期化(URL / 保存値の読み込み)は、マウント後に一度だけ。月を切り替えるたびに URL の月へ戻されないように
  const initialized = useRef(false);

  // URL の ?month= パラメータを読み込む（マウント後）
  useEffect(() => {
    if (!mounted || initialized.current) return;
    initialized.current = true;

    const url = new URL(window.location.href);
    const urlMonth = url.searchParams.get("month");

    if (isValidMonth(urlMonth)) {
      // URL パラメータが優先
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setMonthState(urlMonth);
      storeMonth(urlMonth);
    } else {
      // URL に無い場合は sessionStorage から読む
      const stored = getStoredMonth();
      if (stored) {
        setMonthState(stored);
      } else {
        // 保存値も無ければ当月を保存
        storeMonth(getCurrentMonth());
      }
    }
  }, [mounted]);

  const setMonth = (newMonth: string) => {
    if (isValidMonth(newMonth)) {
      setMonthState(newMonth);
      storeMonth(newMonth);
      // URL も更新（オプション。ここでは更新しない）
    }
  };

  return (
    <MonthContext.Provider value={{ month, setMonth }}>
      {children}
    </MonthContext.Provider>
  );
}

/**
 * 月管理フック
 * @returns { month: "YYYY-MM", setMonth: (month: string) => void }
 */
export function useCurrentMonth(): MonthContextType {
  const context = useContext(MonthContext);
  if (context === undefined) {
    throw new Error("useCurrentMonth must be used within MonthProvider");
  }
  return context;
}
