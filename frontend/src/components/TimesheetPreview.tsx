"use client";

/**
 * TimesheetPreview — 稼働報告プレビュー共通コンポーネント（左右分割）
 *
 * 左ペイン: ExcelはSheetJSで生データ、PDFは埋め込みプレビュー
 * 右ペイン: バックエンド解析結果の日別明細テーブル + サマリー + アラート
 *
 * STAFF画面・ポータル画面の両方で共通利用。
 */

import { cn } from "@/lib/utils";
import { CheckCircle, AlertTriangle, Loader2 } from "lucide-react";

// ── 型定義 ──

export interface DailyEntry {
  date: string;      // "2026-07-01"
  day_name: string;  // "火"
  hours: number;     // 8.0
  start: string;     // "09:00"
  end: string;       // "18:00"
}

export interface TimesheetPreviewData {
  worker_name: string;
  target_month: string | null;
  total_hours: string;
  work_days: number;
  overtime_hours: string;
  night_hours?: string;
  holiday_hours?: string;
  sheet_name: string;
  original_filename: string;
  has_times: boolean;
  alerts: string[];
  daily_data: DailyEntry[];
}

interface TimesheetPreviewProps {
  preview: TimesheetPreviewData;
  /** Excel のときのみ。PDF のときは null */
  excelBuffer?: ArrayBuffer | null;
  /** PDF プレビュー用 Object URL（呼び出し側で revoke） */
  pdfUrl?: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  isConfirming?: boolean;
}

// ── ヘルパー ──

/** 時刻文字列を分に変換 */
function timeToMinutes(time: string): number {
  if (!time || time === "-" || time === "—") return 0;
  const parts = time.split(":");
  return parseInt(parts[0], 10) * 60 + parseInt(parts[1] || "0", 10);
}

/** 休憩時間を逆算（分） = (end - start) - hours * 60 */
function calcBreakMinutes(entry: DailyEntry): number | null {
  if (!entry.start || !entry.end || entry.start === "-" || entry.end === "-") return null;
  const startMin = timeToMinutes(entry.start);
  const endMin = timeToMinutes(entry.end);
  const workMin = (endMin - startMin);
  const hoursMin = entry.hours * 60;
  const breakMin = Math.round(workMin - hoursMin);
  return breakMin > 0 ? breakMin : 0;
}

/** 土日判定 */
function isWeekend(dayName: string): boolean {
  return dayName === "土" || dayName === "日";
}

/** 稼働なし判定 */
function isNoWork(entry: DailyEntry): boolean {
  return entry.hours === 0 && (!entry.start || entry.start === "-" || entry.start === "—");
}

// ── コンポーネント ──

export function TimesheetPreview({ preview, excelBuffer, pdfUrl, onConfirm, onCancel, isConfirming = false }: TimesheetPreviewProps) {
  // Excel の生シート HTML プレビューは廃止（脆弱な xlsx 依存を除去）。
  // 解析結果（右ペイン）で確認する。excelBuffer は後方互換のため受け取るのみ。
  void excelBuffer;

  // サマリーデータ
  const totalHours = parseFloat(preview.total_hours) || 0;
  const overtimeHours = parseFloat(preview.overtime_hours) || 0;

  // 対象月の表示形式
  const targetMonthDisplay = preview.target_month
    ? (() => {
        const d = new Date(preview.target_month);
        return `${d.getFullYear()}年${String(d.getMonth() + 1).padStart(2, "0")}月`;
      })()
    : "—";

  const sourceLabel = pdfUrl ? "PDFプレビュー" : "元データ（Excel）";

  return (
    <div className="space-y-4">
      {/* 左右分割パネル */}
      <div className="grid grid-cols-[2fr_3fr] gap-4 min-h-[500px]">
        {/* 左ペイン: 元ファイル */}
        <div className="bg-card border border-border rounded-lg overflow-hidden flex flex-col">
          <div className="px-3 py-2 border-b border-border bg-muted/30 flex items-center justify-between">
            <span className="text-xs font-medium text-muted-foreground">{sourceLabel}</span>
            {preview.sheet_name && (
              <span className="text-[10px] text-muted-foreground/70 truncate max-w-[140px]">{preview.sheet_name}</span>
            )}
          </div>
          <div className="flex-1 overflow-auto p-2">
            {pdfUrl ? (
              <iframe title="勤務表PDF" src={pdfUrl} className="h-full min-h-[460px] w-full rounded border border-border bg-white" />
            ) : excelBuffer ? (
              <p className="text-xs text-muted-foreground p-3">
                Excel の生シートプレビューは提供しません。右側の解析結果を確認してください。
              </p>
            ) : (
              <p className="text-xs text-muted-foreground p-3">元ファイルのプレビューはありません</p>
            )}
          </div>
        </div>

        {/* 右ペイン: 解析結果 */}
        <div className="bg-card border border-border rounded-lg overflow-hidden flex flex-col">
          <div className="px-4 py-2.5 border-b border-border bg-muted/30">
            <span className="text-xs font-medium text-foreground">解析結果</span>
          </div>

          <div className="flex-1 overflow-auto p-4 space-y-4">
            {/* サマリーカード */}
            <div className="grid grid-cols-4 gap-2 sm:grid-cols-5">
              <SummaryCard label="作業者" value={preview.worker_name || "—"} />
              <SummaryCard label="対象月" value={targetMonthDisplay} />
              <SummaryCard label="合計" value={`${totalHours.toFixed(1)}h`} highlight />
              <SummaryCard label="稼働日数" value={`${preview.work_days}日`} />
              <SummaryCard label="精算超過" value={`${overtimeHours.toFixed(1)}h`} warn={overtimeHours > 0} />
            </div>

            {/* アラート */}
            {preview.alerts.length > 0 && (
              <div className="space-y-1">
                {preview.alerts.map((alert, i) => (
                  <div key={i} className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-amber-500/10 border border-amber-500/20">
                    <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                    <span className="text-xs text-amber-300">{alert}</span>
                  </div>
                ))}
              </div>
            )}

            {/* 日別明細テーブル */}
            <div className="border border-border rounded-lg overflow-hidden">
              <table className="w-full text-sm">
                <thead>
                  <tr className="bg-muted/50 border-b border-border">
                    <th className="px-3 py-2 text-left text-xs font-medium text-muted-foreground">日付</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground w-10">曜日</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">開始</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">終了</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">休憩</th>
                    <th className="px-2 py-2 text-right text-xs font-medium text-muted-foreground">実働時間</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground w-8">✓</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.daily_data.map((entry, i) => {
                    const weekend = isWeekend(entry.day_name);
                    const noWork = isNoWork(entry);
                    const hasAlert = !noWork && weekend && entry.hours > 0;
                    const breakMin = calcBreakMinutes(entry);

                    return (
                      <tr
                        key={i}
                        className={cn(
                          "border-b border-border/50 transition-colors",
                          hasAlert ? "bg-amber-500/5" : noWork && weekend ? "text-muted-foreground/50" : ""
                        )}
                      >
                        <td className="px-3 py-1.5 text-xs tabular-nums">
                          {entry.date.replace(/^\d{4}-/, "")}
                        </td>
                        <td className={cn(
                          "px-2 py-1.5 text-xs text-center",
                          entry.day_name === "土" ? "text-blue-400" : entry.day_name === "日" ? "text-red-400" : ""
                        )}>
                          {entry.day_name}
                        </td>
                        <td className="px-2 py-1.5 text-xs text-center tabular-nums">
                          {noWork ? "—" : entry.start || "—"}
                        </td>
                        <td className="px-2 py-1.5 text-xs text-center tabular-nums">
                          {noWork ? "—" : entry.end || "—"}
                        </td>
                        <td className="px-2 py-1.5 text-xs text-center tabular-nums">
                          {noWork ? "—" : breakMin !== null ? `${breakMin}分` : "—"}
                        </td>
                        <td className="px-2 py-1.5 text-xs text-right tabular-nums font-medium">
                          {noWork ? "—" : `${entry.hours.toFixed(1)}h`}
                        </td>
                        <td className="px-2 py-1.5 text-center">
                          {noWork ? null : hasAlert ? (
                            <AlertTriangle className="w-3.5 h-3.5 text-amber-400 mx-auto" />
                          ) : (
                            <CheckCircle className="w-3.5 h-3.5 text-emerald-400 mx-auto" />
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
                <tfoot>
                  <tr className="bg-muted/30 border-t border-border">
                    <td colSpan={5} className="px-3 py-2 text-xs font-medium text-right text-muted-foreground">合計</td>
                    <td className="px-2 py-2 text-xs text-right tabular-nums font-bold text-foreground">{totalHours.toFixed(1)}h</td>
                    <td />
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
        </div>
      </div>

      {/* アクションボタン */}
      <div className="flex items-center justify-end gap-3">
        <button
          onClick={onCancel}
          className="px-4 py-2 text-sm font-medium text-muted-foreground border border-border rounded-lg hover:bg-accent/50 transition-colors"
        >
          キャンセル
        </button>
        <button
          onClick={onConfirm}
          disabled={isConfirming}
          className="flex items-center gap-2 px-5 py-2 text-sm font-bold text-white bg-gradient-to-r from-emerald-600 to-emerald-500 rounded-lg hover:from-emerald-500 hover:to-emerald-400 shadow-lg shadow-emerald-600/20 transition-all active:scale-95 disabled:opacity-50"
        >
          {isConfirming ? (
            <><Loader2 className="w-4 h-4 animate-spin" /> 登録中...</>
          ) : (
            <><CheckCircle className="w-4 h-4" /> この内容で登録する</>
          )}
        </button>
      </div>
    </div>
  );
}

function SummaryCard({ label, value, highlight, warn }: { label: string; value: string; highlight?: boolean; warn?: boolean }) {
  return (
    <div className={cn(
      "rounded-lg border px-3 py-2",
      warn ? "border-amber-500/30 bg-amber-500/5" : "border-border bg-muted/20"
    )}>
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <p className={cn(
        "text-sm font-bold mt-0.5 tabular-nums",
        highlight ? "text-blue-400" : warn ? "text-amber-400" : "text-foreground"
      )}>{value}</p>
    </div>
  );
}
