"use client";

import { useSyncExternalStore } from "react";

// ハイドレーション完了後だけ true。SSR の HTML とクライアント初回描画を一致させ、
// react-query のキャッシュ(管理者判定など)に依存する UI の hydration mismatch を防ぐ。
export function useMounted() {
  return useSyncExternalStore(
    () => () => {},
    () => true,
    () => false,
  );
}
