"use client";

import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { CalendarClock } from "lucide-react";
import { toast } from "sonner";
import {
  fetchSelfReport,
  submitSelfReport,
  prepareSelfReportSheet,
  submitSelfReportSheet,
  type SelfReportDailyEntry,
} from "@/lib/api";

const STANDARD_START = "09:00";
const STANDARD_END = "18:00";
const STANDARD_BREAK = 60;
const DOW_LABELS = ["日", "月", "火", "水", "木", "金", "土"];

function currentMonthValue(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

function daysInMonth(month: string): number {
  const [y, m] = month.split("-").map(Number);
  return new Date(y, m, 0).getDate();
}

function buildDefaultEntries(month: string): SelfReportDailyEntry[] {
  const [y, m] = month.split("-").map(Number);
  const total = daysInMonth(month);
  const entries: SelfReportDailyEntry[] = [];
  for (let day = 1; day <= total; day++) {
    const dow = new Date(y, m - 1, day).getDay();
    const isWeekend = dow === 0 || dow === 6;
    entries.push({
      day,
      start: isWeekend ? "" : STANDARD_START,
      end: isWeekend ? "" : STANDARD_END,
      break_minutes: isWeekend ? 0 : STANDARD_BREAK,
      off: isWeekend,
    });
  }
  return entries;
}

function toMinutes(hhmm: string): number | null {
  const parts = hhmm.split(":");
  if (parts.length !== 2) return null;
  const h = parseInt(parts[0], 10);
  const m = parseInt(parts[1], 10);
  if (Number.isNaN(h) || Number.isNaN(m)) return null;
  return h * 60 + m;
}

function workedHours(entry: SelfReportDailyEntry): number {
  if (entry.off) return 0;
  const s = toMinutes(entry.start);
  const e = toMinutes(entry.end);
  if (s === null || e === null) return 0;
  const mins = e - s - (entry.break_minutes || 0);
  return mins > 0 ? mins / 60 : 0;
}

function dowOf(month: string, day: number): number {
  const [y, m] = month.split("-").map(Number);
  return new Date(y, m - 1, day).getDay();
}

function summarize(month: string, entries: SelfReportDailyEntry[]) {
  let days = 0, total = 0, overtime = 0, holiday = 0;
  for (const entry of entries) {
    const worked = workedHours(entry);
    if (worked <= 0) continue;
    days += 1;
    total += worked;
    if (worked > 8) overtime += worked - 8;
    const dow = dowOf(month, entry.day);
    if (dow === 0 || dow === 6) holiday += worked;
  }
  return { days, total, overtime, holiday };
}

export default function MyTimesheetPage() {
  const qc = useQueryClient();
  const [targetMonth, setTargetMonth] = useState(currentMonthValue());
  const [tab, setTab] = useState<"grid" | "sheets">("grid");
  const [entries, setEntries] = useState<SelfReportDailyEntry[]>(() => buildDefaultEntries(currentMonthValue()));
  const [nightHours, setNightHours] = useState("0");
  const [submitting, setSubmitting] = useState(false);
  const [sheetsBusy, setSheetsBusy] = useState<"prepare" | "submit" | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["self-report", targetMonth],
    queryFn: () => fetchSelfReport(targetMonth),
  });

  const sheet = data?.sheet ?? null;
  const isApproved = sheet?.status === "APPROVED";
  const isSubmitted = !!sheet && sheet.status !== "PENDING";

  // sheetデータ（対象月・提出内容）が変わったタイミングだけ画面の入力状態を作り直す。
  // レンダー中に条件付きでsetStateする公式パターン（useEffectでpropsをstateへコピーしない）
  const loadedKey = `${targetMonth}:${sheet?.id ?? "new"}`;
  const [lastLoadedKey, setLastLoadedKey] = useState<string | null>(null);
  if (loadedKey !== lastLoadedKey) {
    setLastLoadedKey(loadedKey);
    if (sheet?.daily_data && sheet.daily_data.length > 0) {
      setEntries(sheet.daily_data);
      setNightHours(sheet.night_hours ?? "0");
    } else {
      setEntries(buildDefaultEntries(targetMonth));
      setNightHours("0");
    }
  }

  const summary = useMemo(() => summarize(targetMonth, entries), [targetMonth, entries]);

  const updateEntry = (day: number, patch: Partial<SelfReportDailyEntry>) => {
    setEntries((prev) => prev.map((e) => (e.day === day ? { ...e, ...patch } : e)));
  };

  const toggleOff = (day: number) => {
    setEntries((prev) => prev.map((e) => {
      if (e.day !== day) return e;
      const off = !e.off;
      if (!off && !e.start) {
        return { ...e, off, start: STANDARD_START, end: STANDARD_END, break_minutes: STANDARD_BREAK };
      }
      return { ...e, off };
    }));
  };

  const resetWeekdays = () => {
    setEntries((prev) => prev.map((e) => (e.off ? e : { ...e, start: STANDARD_START, end: STANDARD_END, break_minutes: STANDARD_BREAK })));
  };

  const handleSubmitGrid = async () => {
    setSubmitting(true);
    try {
      const res = await submitSelfReport({
        target_month: `${targetMonth}-01`,
        daily_data: entries,
        night_hours: Number(nightHours) || 0,
      });
      toast.success(res.message || "提出しました");
      qc.invalidateQueries({ queryKey: ["self-report", targetMonth] });
    } catch (e) {
      toast.error(`提出に失敗しました: ${e instanceof Error ? e.message : e}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleOpenSheet = async () => {
    setSheetsBusy("prepare");
    try {
      const res = await prepareSelfReportSheet(`${targetMonth}-01`);
      if (res.url) {
        window.open(res.url, "_blank", "noopener,noreferrer");
        qc.invalidateQueries({ queryKey: ["self-report", targetMonth] });
      } else {
        toast.error(res.error || "スプレッドシートの準備に失敗しました");
      }
    } catch (e) {
      toast.error(`スプレッドシートの準備に失敗しました: ${e instanceof Error ? e.message : e}`);
    } finally {
      setSheetsBusy(null);
    }
  };

  const handleFetchSheet = async () => {
    setSheetsBusy("submit");
    try {
      const res = await submitSelfReportSheet(`${targetMonth}-01`);
      toast.success(res.message || "取り込みました");
      qc.invalidateQueries({ queryKey: ["self-report", targetMonth] });
    } catch (e) {
      toast.error(`取り込みに失敗しました: ${e instanceof Error ? e.message : e}`);
    } finally {
      setSheetsBusy(null);
    }
  };

  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
          <CalendarClock className="w-5 h-5 text-sky-400" /> 稼働報告（自己申告）
        </h1>
        <p className="text-xs text-muted-foreground mt-1">
          受注契約が無い社員向けの月次稼働報告です。見慣れた勤務表と同じ列構成で入力できます。
        </p>
      </div>

      {isSubmitted && (
        <div className={`flex items-center gap-2 px-3 py-2 rounded-md text-xs font-medium border ${
          isApproved
            ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
            : "bg-amber-500/10 text-amber-400 border-amber-500/30"
        }`}>
          <span className="w-1.5 h-1.5 rounded-full bg-current shrink-0" />
          {isApproved
            ? "この月の内容は承認済みです。再編集はできません。"
            : "提出済み（承認待ち）です。管理者が承認するまで給与計算には反映されません。"}
        </div>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="outline" size="sm"
          onClick={() => {
            const [y, m] = targetMonth.split("-").map(Number);
            const d = new Date(y, m - 2, 1);
            setTargetMonth(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`);
          }}
        >‹</Button>
        <span className="text-sm font-semibold tabular-nums min-w-[100px] text-center">
          {targetMonth.replace("-", "年")}月
        </span>
        <Button
          variant="outline" size="sm"
          onClick={() => {
            const [y, m] = targetMonth.split("-").map(Number);
            const d = new Date(y, m, 1);
            setTargetMonth(`${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`);
          }}
        >›</Button>
      </div>

      <div className="flex gap-4 border-b border-border">
        <button
          onClick={() => setTab("grid")}
          className={`text-sm font-medium pb-2 border-b-2 -mb-px ${tab === "grid" ? "text-foreground border-sky-400" : "text-muted-foreground border-transparent hover:text-foreground"}`}
        >画面で入力</button>
        <button
          onClick={() => setTab("sheets")}
          className={`text-sm font-medium pb-2 border-b-2 -mb-px ${tab === "sheets" ? "text-foreground border-sky-400" : "text-muted-foreground border-transparent hover:text-foreground"}`}
        >Googleスプレッドシートで入力</button>
      </div>

      {tab === "grid" && (
        <>
          <div className="grid grid-cols-2 sm:grid-cols-5 gap-px bg-border border border-border rounded-lg overflow-hidden">
            {[
              { label: "稼働日数", value: `${summary.days}日` },
              { label: "総稼働時間", value: `${summary.total.toFixed(1)}h` },
              { label: "残業時間", value: `${summary.overtime.toFixed(1)}h` },
              { label: "休日出勤", value: `${summary.holiday.toFixed(1)}h` },
              { label: "深夜時間", value: `${nightHours || 0}h` },
            ].map((s) => (
              <div key={s.label} className="bg-card px-3 py-2.5">
                <div className="text-[10px] text-muted-foreground uppercase tracking-wide mb-1">{s.label}</div>
                <div className="text-lg font-bold tabular-nums text-foreground">{s.value}</div>
              </div>
            ))}
          </div>

          <div className="flex items-center justify-between flex-wrap gap-2">
            <p className="text-xs text-muted-foreground">
              平日は 9:00–18:00 / 休憩60分を自動入力済みです。例外の日だけ直接編集してください。
            </p>
            <Button variant="outline" size="sm" onClick={resetWeekdays} disabled={isApproved}>
              平日をすべて 9:00–18:00 にリセット
            </Button>
          </div>

          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">日</TableHead>
                  <TableHead className="text-xs text-muted-foreground">曜日</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-center">開始</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-center">終了</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-center">休憩(分)</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-center">休み</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">実働(h)</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {entries.map((entry) => {
                  const dow = dowOf(targetMonth, entry.day);
                  const isWeekend = dow === 0 || dow === 6;
                  const worked = workedHours(entry);
                  return (
                    <TableRow key={entry.day} className={`border-border/50 ${isWeekend ? "bg-amber-500/5" : ""}`}>
                      <TableCell className="text-sm tabular-nums font-medium">{entry.day}</TableCell>
                      <TableCell className={`text-sm ${dow === 0 ? "text-red-400" : dow === 6 ? "text-sky-400" : "text-muted-foreground"}`}>
                        {DOW_LABELS[dow]}
                      </TableCell>
                      <TableCell className="text-center">
                        <input
                          type="text"
                          value={entry.start}
                          disabled={entry.off || isApproved}
                          onChange={(e) => updateEntry(entry.day, { start: e.target.value })}
                          className="w-16 text-center text-xs tabular-nums bg-muted border border-border rounded px-1 py-1 disabled:opacity-40"
                        />
                      </TableCell>
                      <TableCell className="text-center">
                        <input
                          type="text"
                          value={entry.end}
                          disabled={entry.off || isApproved}
                          onChange={(e) => updateEntry(entry.day, { end: e.target.value })}
                          className="w-16 text-center text-xs tabular-nums bg-muted border border-border rounded px-1 py-1 disabled:opacity-40"
                        />
                      </TableCell>
                      <TableCell className="text-center">
                        <input
                          type="text"
                          inputMode="numeric"
                          value={entry.break_minutes}
                          disabled={entry.off || isApproved}
                          onChange={(e) => updateEntry(entry.day, { break_minutes: parseInt(e.target.value, 10) || 0 })}
                          className="w-12 text-center text-xs tabular-nums bg-muted border border-border rounded px-1 py-1 disabled:opacity-40"
                        />
                      </TableCell>
                      <TableCell className="text-center">
                        <input
                          type="checkbox"
                          checked={entry.off}
                          disabled={isApproved}
                          onChange={() => toggleOff(entry.day)}
                          className="rounded border-border"
                        />
                      </TableCell>
                      <TableCell className="text-right text-sm tabular-nums font-medium">
                        {entry.off ? "—" : worked.toFixed(1)}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>

          <div className="flex items-center gap-3">
            <label className="text-xs text-muted-foreground">深夜時間（22時以降勤務がある場合のみ）</label>
            <input
              type="text"
              inputMode="decimal"
              value={nightHours}
              disabled={isApproved}
              onChange={(e) => setNightHours(e.target.value)}
              className="w-20 text-center text-sm bg-muted border border-border rounded px-2 py-1 disabled:opacity-40"
            />
            <span className="text-xs text-muted-foreground">h</span>
          </div>

          <div className="flex items-center justify-between bg-card border border-border rounded-lg p-4 flex-wrap gap-3">
            <p className="text-xs text-muted-foreground">
              <strong className="text-foreground">{targetMonth.replace("-", "年")}月分</strong>として提出します。提出後も承認前であれば再編集できます。
            </p>
            <Button onClick={handleSubmitGrid} disabled={submitting || isApproved || isLoading}>
              {isApproved ? "承認済み" : submitting ? "提出中…" : "この内容で提出する"}
            </Button>
          </div>
        </>
      )}

      {tab === "sheets" && (
        <div className="space-y-4">
          <div className="grid sm:grid-cols-2 gap-4">
            <div className="bg-card border border-border rounded-lg p-5 space-y-2">
              <h3 className="text-sm font-semibold text-foreground">① スプレッドシートを開く</h3>
              <p className="text-xs text-muted-foreground leading-relaxed">
                画面入力とまったく同じ列構成のGoogleスプレッドシートが、あなた専用のコピーとして新しいタブで開きます。
              </p>
              <Button size="sm" onClick={handleOpenSheet} disabled={sheetsBusy !== null || isApproved}>
                {sheetsBusy === "prepare" ? "準備中…" : `${targetMonth.replace("-", "年")}月分のスプレッドシートを開く`}
              </Button>
              {sheet?.sheet_file_id && (
                <p className="text-[11px] text-emerald-400">✓ 作成済み</p>
              )}
            </div>
            <div className="bg-card border border-border rounded-lg p-5 space-y-2">
              <h3 className="text-sm font-semibold text-foreground">② 入力が終わったら取り込む</h3>
              <p className="text-xs text-muted-foreground leading-relaxed">
                スプレッドシート側で入力・保存した内容を、こちらのボタンでSophiaに取り込みます。何度でも取り込み直せます。
              </p>
              <Button
                size="sm" variant="outline"
                onClick={handleFetchSheet}
                disabled={sheetsBusy !== null || !sheet?.sheet_file_id || isApproved}
              >
                {sheetsBusy === "submit" ? "取り込み中…" : "スプレッドシートの内容を取り込む"}
              </Button>
            </div>
          </div>
          {sheet && sheet.status !== "PENDING" && (
            <div className="bg-card border border-border rounded-lg p-4 text-xs text-muted-foreground">
              取り込み済み: 稼働日数 {sheet.work_days}日 / 総稼働時間 {parseFloat(sheet.total_hours).toFixed(1)}h / 残業 {parseFloat(sheet.overtime_hours).toFixed(1)}h / 休日出勤 {parseFloat(sheet.holiday_hours).toFixed(1)}h
            </div>
          )}
        </div>
      )}
    </div>
  );
}
