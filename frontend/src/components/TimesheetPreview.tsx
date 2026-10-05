"use client";

/**
 * TimesheetPreview — 稼働報告プレビュー共通コンポーネント（左右分割）
 *
 * 左ペイン: ExcelはSheetJSで生データ、PDFは埋め込みプレビュー
 * 右ペイン: バックエンド解析結果の日別明細テーブル + サマリー + アラート
 *
 * STAFF画面・ポータル画面の両方で共通利用。
 */

import { useMemo, type ReactNode } from "react";
import { read, utils } from "xlsx";
import { cn } from "@/lib/utils";
import { sanitizeTableHtml } from "@/lib/sanitize-html";
import { CheckCircle, AlertTriangle, Loader2 } from "lucide-react";

// ── 型定義 ──

export interface DailyEntry {
  date: string;      // "2026-07-01"
  day_name: string;  // "火"
  hours: number;     // 8.0（PDFの勤務表では、アプリが開始・終了・休憩から計算した実働時間）
  start: string;     // "09:00"
  end: string;       // "18:00"
  /** 勤務表に書かれた休憩（分）。読み取れない書式では null / 未指定 */
  break_minutes?: number | null;
  /** 勤務表に書かれた実働時間（比較用）。hours と合わない日は警告する */
  stated_hours?: number | null;
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
  /** 祝日の日付（"2026-09-21" 形式）。土日と同じく背景色を変える。勤務表に「祝」と書かれた日も祝日として扱う */
  holiday_dates?: string[];
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
  /** 確認ボタンの文言（既定: 「この内容で登録する」） */
  confirmLabel?: string;
  /** false のとき確認ボタンを押せなくする（既定: 押せる） */
  canConfirm?: boolean;
  /** 左右パネルの上に差し込む欄（メール取込での「結び付ける受注」など） */
  header?: ReactNode;
  /** 下のボタン行の、キャンセルの前に差し込むボタン（メール取込での「差し戻し」など） */
  extraActions?: ReactNode;
  /** true のとき、親の高さいっぱいに収める（スクロールなしで1画面のスクリーンショットに収めたい用途）。
   *  重要なのは右側（アプリの計算値）。右側を広くとり、全日付を行間を詰めて表示する。
   *  左の原本（PDF）は比較用なので、幅を絞って1ページ全体を表示する */
  fit?: boolean;
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

export function TimesheetPreview({ preview, excelBuffer, pdfUrl, onConfirm, onCancel, isConfirming = false, confirmLabel, canConfirm = true, header, extraActions, fit = false }: TimesheetPreviewProps) {
  // Excel生データをHTMLテーブルに変換（XSS対策: sanitizeTableHtml で無害化）
  const excelHtml = useMemo(() => {
    if (!excelBuffer) return null;
    try {
      const wb = read(excelBuffer, { type: "array" });
      // 解析対象シートを探す（sheet_nameが一致するもの、なければ最初のシート）
      const sheetName = preview.sheet_name && wb.SheetNames.includes(preview.sheet_name)
        ? preview.sheet_name
        : wb.SheetNames[0];
      const ws = wb.Sheets[sheetName];
      const rawHtml = utils.sheet_to_html(ws, { id: "excel-preview", header: "", footer: "" });
      // Sanitize HTML to remove XSS vectors (script, iframe, event handlers, etc.)
      const sanitizedHtml = sanitizeTableHtml(rawHtml);
      return {
        html: sanitizedHtml,
        sheetName,
        sheetNames: wb.SheetNames,
      };
    } catch {
      return { html: "<p>Excelファイルの表示に失敗しました</p>", sheetName: "", sheetNames: [] };
    }
  }, [excelBuffer, preview.sheet_name]);

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
    <div className={fit ? "flex flex-col h-full min-h-0 gap-2" : "space-y-4"}>
      {header && <div className="shrink-0">{header}</div>}
      {/* 左右分割パネル */}
      <div className={fit ? "grid grid-cols-[1fr_2fr] gap-3 flex-1 min-h-0" : "grid grid-cols-[2fr_3fr] gap-4 min-h-[500px]"}>
        {/* 左ペイン: 元ファイル */}
        <div className="bg-card border border-border rounded-lg overflow-hidden flex flex-col min-h-0">
          <div className="px-3 py-2 border-b border-border bg-muted/30 flex items-center justify-between">
            <span className="text-xs font-medium text-muted-foreground">{sourceLabel}</span>
            {excelHtml?.sheetName && (
              <span className="text-[10px] text-muted-foreground/70 truncate max-w-[140px]">{excelHtml.sheetName}</span>
            )}
          </div>
          {/* PDFは1ページ全体を枠に収める。Excelは、セルを折り返さず（「時間」が2行になって1か月分が入らなくなるため）、横にスクロールする */}
          <div className={fit ? (pdfUrl ? "flex-1 min-h-0 overflow-hidden p-1" : "flex-1 min-h-0 overflow-auto p-1") : "flex-1 overflow-auto p-2"}>
            {pdfUrl ? (
              <iframe
                title="勤務表PDF"
                src={fit ? `${pdfUrl}#view=Fit&toolbar=0` : pdfUrl}
                className={fit ? "h-full w-full rounded border border-border bg-white" : "h-full min-h-[460px] w-full rounded border border-border bg-white"}
              />
            ) : excelHtml ? (
              <div
                className={cn(
                  "excel-preview-table text-[10px] leading-tight [&_table]:w-full [&_td]:border [&_td]:border-border/40 [&_td]:px-1 [&_td]:py-0.5 [&_th]:border [&_th]:border-border/40 [&_th]:px-1 [&_th]:py-0.5",
                  fit && "[&_td]:whitespace-nowrap [&_th]:whitespace-nowrap [&_td]:py-0 [&_td]:leading-[15px]"
                )}
                dangerouslySetInnerHTML={{ __html: excelHtml.html }}
              />
            ) : (
              <p className="text-xs text-muted-foreground p-3">元ファイルのプレビューはありません</p>
            )}
          </div>
        </div>

        {/* 右ペイン: 解析結果 */}
        <div className="bg-card border border-border rounded-lg overflow-hidden flex flex-col min-h-0">
          <div className="px-4 py-2.5 border-b border-border bg-muted/30">
            <span className="text-xs font-medium text-foreground">解析結果</span>
          </div>

          <div className={fit ? "flex-1 min-h-0 overflow-auto p-2 space-y-1.5" : "flex-1 overflow-auto p-4 space-y-4"}>
            {/* サマリーカード */}
            <div className="grid grid-cols-4 gap-2 sm:grid-cols-5">
              <SummaryCard compact={fit} label="作業者" value={preview.worker_name || "—"} />
              <SummaryCard compact={fit} label="対象月" value={targetMonthDisplay} />
              <SummaryCard compact={fit} label="合計（計算）" value={`${totalHours.toFixed(2).replace(/\.?0+$/, "")}h`} highlight />
              <SummaryCard compact={fit} label="稼働日数" value={`${preview.work_days}日`} />
              <SummaryCard compact={fit} label="精算超過" value={`${overtimeHours.toFixed(1)}h`} warn={overtimeHours > 0} />
            </div>

            {/* アラート */}
            {preview.alerts.length > 0 && (
              <div className="space-y-1">
                {preview.alerts.map((alert, i) => (
                  <div key={i} className={cn("flex items-center gap-2 px-3 rounded-md bg-amber-500/10 border border-amber-500/20", fit ? "py-0.5" : "py-1.5")}>
                    <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                    <span className="text-xs text-amber-300">{alert}</span>
                  </div>
                ))}
              </div>
            )}

            {/* 日別明細テーブル */}
            <div className="border border-border rounded-lg overflow-hidden">
              <table className={cn("w-full text-sm", fit && "[&_td]:py-0 [&_td]:text-[11px] [&_td]:leading-[18px] [&_th]:py-0.5 [&_th]:text-[11px]")}>
                <thead>
                  <tr className="bg-muted/50 border-b border-border">
                    <th className="px-3 py-2 text-left text-xs font-medium text-muted-foreground">日付</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground w-10">曜日</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">開始</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">終了</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground">休憩</th>
                    <th className="px-2 py-2 text-right text-xs font-medium text-muted-foreground">実働時間（計算）</th>
                    <th className="px-2 py-2 text-right text-xs font-medium text-muted-foreground">勤務表の記載</th>
                    <th className="px-2 py-2 text-center text-xs font-medium text-muted-foreground w-8">✓</th>
                  </tr>
                </thead>
                <tbody>
                  {preview.daily_data.map((entry, i) => {
                    const weekend = isWeekend(entry.day_name);
                    const noWork = isNoWork(entry);
                    const hasAlert = !noWork && weekend && entry.hours > 0;
                    // 休憩は勤務表の記載を使う。無い書式では、終了−開始−実働から逆算する
                    const breakMin = entry.break_minutes ?? calcBreakMinutes(entry);
                    // アプリの計算値が、勤務表の記載と合わない日
                    const hoursDiffer = entry.stated_hours != null && Math.abs(entry.stated_hours - entry.hours) > 0.01;
                    const holiday = entry.day_name === "祝" || (preview.holiday_dates?.includes(entry.date) ?? false);
                    // 土は青、日・祝日は赤の背景にして、休みの日をひと目で分かるようにする
                    const dayTint = entry.day_name === "土" ? "bg-blue-500/10" : entry.day_name === "日" || holiday ? "bg-red-500/10" : "";

                    return (
                      <tr
                        key={i}
                        className={cn(
                          "border-b border-border/50 transition-colors",
                          dayTint,
                          noWork && (weekend || holiday) ? "text-muted-foreground/70" : ""
                        )}
                      >
                        <td className="px-3 py-1.5 text-xs tabular-nums">
                          {entry.date.replace(/^\d{4}-/, "")}
                        </td>
                        <td className={cn(
                          "px-2 py-1.5 text-xs text-center",
                          entry.day_name === "土" ? "text-blue-400" : entry.day_name === "日" || holiday ? "text-red-400" : ""
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
                        <td className={cn("px-2 py-1.5 text-xs text-right tabular-nums font-medium", hoursDiffer && "text-amber-400")}>
                          {noWork ? "—" : `${entry.hours.toFixed(2).replace(/\.?0+$/, "")}h`}
                        </td>
                        <td className={cn("px-2 py-1.5 text-xs text-right tabular-nums", hoursDiffer ? "text-amber-400 font-medium" : "text-muted-foreground")}>
                          {noWork || entry.stated_hours == null ? "—" : `${entry.stated_hours.toFixed(2).replace(/\.?0+$/, "")}h`}
                        </td>
                        <td className="px-2 py-1.5 text-center" title={hoursDiffer ? "計算した実働時間が、勤務表の記載と違います" : hasAlert ? "土日の稼働です" : undefined}>
                          {noWork ? null : hasAlert || hoursDiffer ? (
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
                    <td className="px-2 py-2 text-xs text-right tabular-nums font-bold text-foreground">{totalHours.toFixed(2).replace(/\.?0+$/, "")}h</td>
                    <td colSpan={2} />
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
        </div>
      </div>

      {/* アクションボタン */}
      <div className="flex items-center justify-end gap-3 shrink-0">
        {extraActions}
        <button
          onClick={onCancel}
          className="px-4 py-2 text-sm font-medium text-muted-foreground border border-border rounded-lg hover:bg-accent/50 transition-colors"
        >
          キャンセル
        </button>
        <button
          onClick={onConfirm}
          disabled={isConfirming || !canConfirm}
          className="flex items-center gap-2 px-5 py-2 text-sm font-bold text-white bg-gradient-to-r from-emerald-600 to-emerald-500 rounded-lg hover:from-emerald-500 hover:to-emerald-400 shadow-lg shadow-emerald-600/20 transition-all active:scale-95 disabled:opacity-50"
        >
          {isConfirming ? (
            <><Loader2 className="w-4 h-4 animate-spin" /> 登録中...</>
          ) : (
            <><CheckCircle className="w-4 h-4" /> {confirmLabel ?? "この内容で登録する"}</>
          )}
        </button>
      </div>
    </div>
  );
}

function SummaryCard({ label, value, highlight, warn, compact }: { label: string; value: string; highlight?: boolean; warn?: boolean; compact?: boolean }) {
  return (
    <div className={cn(
      "rounded-lg border",
      compact ? "px-2 py-1" : "px-3 py-2",
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
