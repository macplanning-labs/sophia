// アサイン編成画面の表示用の整形(純関数)。金額・粗利率はサーバーの値をそのまま整形するだけで、再計算しない。

export function formatYen(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  const sign = n < 0 ? "-" : "";
  return `${sign}¥${Math.abs(n).toLocaleString("ja-JP")}`;
}

export function formatPct(p: number | null | undefined): string {
  return p === null || p === undefined ? "—" : `${p.toFixed(1)}%`;
}

export function flowLabel(flow: string | null | undefined): string {
  if (flow === "DIRECT") return "直受け";
  if (flow === "SUBCONTRACT") return "下請け";
  return "商流未設定";
}

export type Tone = "ok" | "warn" | "muted";

/** 目安との比較の表示(色の系統と文言) */
export function targetBadge(
  status: string,
  diffPct: number | null | undefined,
  targetPct: number | null | undefined,
): { tone: Tone; text: string } {
  switch (status) {
    case "OK":
      return { tone: "ok", text: `目安${targetPct}% 以上` };
    case "BELOW":
      return { tone: "warn", text: `目安${targetPct}% を${Math.abs(diffPct ?? 0).toFixed(1)}pt 下回る` };
    case "UNSET":
      return { tone: "muted", text: "商流を設定すると目安を表示" };
    default:
      return { tone: "muted", text: "—" };
  }
}

/** 警告の重さ。赤字は強く、それ以外は注意 */
export function warningTone(code: string): "danger" | "caution" {
  return code === "LOSS" ? "danger" : "caution";
}
