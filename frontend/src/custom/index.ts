// カスタマイズが無いとき（公開版）の差し込み口。公開版では、このファイルの内容で index.ts を置き換える。
// 社内でも型検査の対象にして、差し込み口の形（CustomExports）とずれないようにしている。
import type { CustomExports } from "./types";

export const custom: CustomExports = { EdiPanel: null };
