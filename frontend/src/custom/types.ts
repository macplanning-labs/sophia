import type { ComponentType } from "react";

/**
 * 取引先ごとのカスタマイズが画面に足す部品（差し込み口）。
 * 汎用の画面は `@/custom` の `custom` だけを使う。カスタマイズが無い公開版では、`none.ts` が `index.ts` の代わりになる。
 */
export type CustomExports = {
  /** トップ画面に出す、取引先 EDI のパネル（無ければ出さない） */
  EdiPanel: ComponentType | null;
};
