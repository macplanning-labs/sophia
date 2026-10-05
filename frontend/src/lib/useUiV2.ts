/**
 * useUiV2.ts — 新UI 切り替え状態管理フック
 *
 * 【決定】§1-6: localStorage キー `sophia-ui-v2` で個人単位の有効化状態を管理。
 * SSR との hydration mismatch を防ぐため useSyncExternalStore を使用。
 * （サーバー描画時は常に false、マウント後に localStorage を読む）
 */

import { useSyncExternalStore } from "react";

const STORAGE_KEY = "sophia-ui-v2";

/**
 * localStorage から V2 UI の有効化状態を取得
 * 例外時は握りつぶし、false を返す
 */
function getUiV2Enabled(): boolean {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === "1";
  } catch {
    // localStorage へのアクセスが失敗した場合（プライベートモード等）
    return false;
  }
}

/**
 * localStorage に V2 UI の有効化状態を設定
 * 例外時は握りつぶす（ログアウト時には呼ばないため、通常は例外は発生しない）
 */
function setUiV2Enabled(enabled: boolean): void {
  try {
    if (enabled) {
      localStorage.setItem(STORAGE_KEY, "1");
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    // 握りつぶす
  }
}

// グローバルリスナー管理（複数の useUiV2 フックから共有）
const listeners: Set<() => void> = new Set();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function emitChange(): void {
  listeners.forEach((listener) => listener());
}

/**
 * useUiV2 - 新UI 有効化状態フック
 *
 * @returns { enabled: boolean, setEnabled: (value: boolean) => void }
 *
 * SSR安全性:
 * - useSyncExternalStore により、サーバー描画時は常に false
 * - クライアント初回マウント後に localStorage を読んで同期
 * - hydration mismatch が発生しない
 *
 * localStorage へのアクセスは try/catch で例外を握りつぶす
 */
export function useUiV2(): { enabled: boolean; setEnabled: (value: boolean) => void } {
  const enabled = useSyncExternalStore(
    subscribe,
    getUiV2Enabled,
    () => false, // サーバー側は常に false（hydration 時）
  );

  return {
    enabled,
    setEnabled: (value: boolean) => {
      setUiV2Enabled(value);
      emitChange(); // リスナーに通知（複数の useUiV2 フックを同期）
    },
  };
}
