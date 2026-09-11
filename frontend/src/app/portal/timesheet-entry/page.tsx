"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import { toast } from "sonner";
import { fetchPortalEngineers, fetchPortalWorkEntries, fetchPortalWorkEntriesSummary, submitPortalWorkEntries, invitePortalEngineer } from "@/lib/api";

// ── 型定義 ──

interface Engineer {
  id: number;
  name: string;
}

interface WorkEntry {
  id?: number;
  engineer_id: number;
  work_date: string;
  start_time: string | null;
  end_time: string | null;
  break_minutes: number;
  actual_minutes: number;
  work_description: string;
  is_holiday: boolean;
}

interface Summary {
  work_days: number;
  total_minutes: number;
  total_hours: string;
  total_display: string;
  missing_dates: string[];
  missing_count: number;
}

interface DayEntry {
  date: string;        // "YYYY-MM-DD"
  dayOfWeek: number;   // 0=Sun, 6=Sat
  dayLabel: string;    // "月", "火" etc
  isWeekend: boolean;
  start_time: string;
  end_time: string;
  break_minutes: number;
  work_description: string;
  is_holiday: boolean;
  actual_minutes: number;
  saved: boolean;      // サーバーに保存済みか
  dirty: boolean;      // 変更あり
}

const DAY_LABELS = ["日", "月", "火", "水", "木", "金", "土"];

// 時間選択肢
const HOURS = Array.from({ length: 25 }, (_, i) => i); // 0〜24
const MINUTES = ["00", "15", "30", "45"];



// ── ヘルパー ──

function calcActualMinutes(start: string, end: string, breakMin: number, isHoliday: boolean): number {
  if (isHoliday || !start || !end) return 0;
  const [sh, sm] = start.split(":").map(Number);
  const [eh, em] = end.split(":").map(Number);
  const work = (eh * 60 + em) - (sh * 60 + sm);
  return Math.max(work - breakMin, 0);
}

function formatMinutes(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

function getDaysInMonth(year: number, month: number): DayEntry[] {
  const days: DayEntry[] = [];
  const daysInMonth = new Date(year, month, 0).getDate();
  for (let d = 1; d <= daysInMonth; d++) {
    const date = new Date(year, month - 1, d);
    const dow = date.getDay();
    days.push({
      date: `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`,
      dayOfWeek: dow,
      dayLabel: DAY_LABELS[dow],
      isWeekend: dow === 0 || dow === 6,
      start_time: "",
      end_time: "",
      break_minutes: 60,
      work_description: "",
      is_holiday: false,
      actual_minutes: 0,
      saved: false,
      dirty: false,
    });
  }
  return days;
}

// ── 時間セレクト（時 + 分の2連） ──

function TimeSelect({ value, onChange, disabled }: {
  value: string;
  onChange: (v: string) => void;
  disabled?: boolean;
}) {
  const [h, m] = value ? value.split(":") : ["", ""];
  const selClass = "bg-muted border border-border rounded px-1 py-1 text-sm text-foreground text-center disabled:opacity-30 appearance-none cursor-pointer hover:border-primary/50 transition-colors";

  const handleChange = (newH: string, newM: string) => {
    if (newH && newM) {
      onChange(`${newH.padStart(2, "0")}:${newM}`);
    } else if (!newH && !newM) {
      onChange("");
    }
  };

  return (
    <div className="flex items-center gap-1 justify-center">
      <div className="relative">
        <select value={h ? parseInt(h).toString() : ""} onChange={(e) => handleChange(e.target.value, m || "00")}
          disabled={disabled} className={`${selClass} w-10`}>
          <option value="">--</option>
          {HOURS.map((hr) => <option key={hr} value={hr}>{hr}</option>)}
        </select>
      </div>
      <span className="text-muted-foreground font-bold">:</span>
      <div className="relative">
        <select value={m} onChange={(e) => handleChange(h || "09", e.target.value)}
          disabled={disabled} className={`${selClass} w-10`}>
          <option value="">--</option>
          {MINUTES.map((mn) => <option key={mn} value={mn}>{mn}</option>)}
        </select>
      </div>
    </div>
  );
}

// ── メインコンポーネント ──

export default function TimesheetEntryPage() {
  const now = new Date();
  const [year, setYear] = useState(now.getFullYear());
  const [month, setMonth] = useState(now.getMonth() + 1);
  const [engineers, setEngineers] = useState<Engineer[]>([]);
  const [selectedEngineer, setSelectedEngineer] = useState<number | null>(null);
  const [days, setDays] = useState<DayEntry[]>([]);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [locked, setLocked] = useState(false);
  const [inviting, setInviting] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  // エンジニア一覧取得
  useEffect(() => {
    fetchPortalEngineers()
      .then((data) => {
        const list = data.engineers || [];
        setEngineers(list);
        if (list.length > 0 && !selectedEngineer) {
          setSelectedEngineer(list[0].id);
        }
      })
      .catch(() => toast.error("エンジニア一覧の取得に失敗しました"))
      .finally(() => setLoading(false));
  }, []);

  // 日次データ取得
  const fetchEntries = useCallback(async () => {
    if (!selectedEngineer) return;
    setLoading(true);
    try {
      const [entriesData, summaryData] = await Promise.all([
        fetchPortalWorkEntries({ engineer_id: selectedEngineer.toString(), year, month }),
        fetchPortalWorkEntriesSummary({ engineer_id: selectedEngineer.toString(), year, month }),
      ]);

      const newDays = getDaysInMonth(year, month);
      const entryMap = new Map<string, WorkEntry>();
      (entriesData.entries || []).forEach((e: WorkEntry) => {
        entryMap.set(e.work_date, e);
      });

      for (const day of newDays) {
        const entry = entryMap.get(day.date);
        if (entry) {
          day.start_time = entry.start_time?.substring(0, 5) || "";
          day.end_time = entry.end_time?.substring(0, 5) || "";
          day.break_minutes = entry.break_minutes;
          day.work_description = entry.work_description;
          day.is_holiday = entry.is_holiday;
          day.actual_minutes = entry.actual_minutes;
          day.saved = true;
        }
      }

      setDays(newDays);
      setSummary(summaryData);
      setLocked(entriesData.locked || false);
    } catch {
      toast.error("稼働データの取得に失敗しました");
    } finally {
      setLoading(false);
    }
  }, [selectedEngineer, year, month]);

  useEffect(() => {
    // エンジニア未選択でもカレンダーグリッドは表示する
    setDays(getDaysInMonth(year, month));
    fetchEntries();
  }, [fetchEntries]);

  // 日次データ更新
  const updateDay = (index: number, field: keyof DayEntry, value: string | number | boolean) => {
    setDays((prev) => {
      const next = [...prev];
      const day = { ...next[index], [field]: value, dirty: true };
      // 実働時間の自動計算
      if (field === "start_time" || field === "end_time" || field === "break_minutes" || field === "is_holiday") {
        day.actual_minutes = calcActualMinutes(
          field === "start_time" ? String(value) : day.start_time,
          field === "end_time" ? String(value) : day.end_time,
          field === "break_minutes" ? Number(value) : day.break_minutes,
          field === "is_holiday" ? Boolean(value) : day.is_holiday,
        );
      }
      next[index] = day;
      return next;
    });
  };

  // 一括保存
  const handleSave = async () => {
    if (!selectedEngineer) return;
    const dirtyDays = days.filter((d) => d.dirty);
    if (dirtyDays.length === 0) {
      toast.info("変更がありません");
      return;
    }
    setSaving(true);
    try {
      const data = await submitPortalWorkEntries({
        engineer_id: selectedEngineer,
        entries: dirtyDays.map((d) => ({
          work_date: d.date,
          start_time: d.start_time || null,
          end_time: d.end_time || null,
          break_minutes: d.break_minutes,
          work_description: d.work_description,
          is_holiday: d.is_holiday,
        })),
      });
      if (data.ok) {
        toast.success(data.message);
        fetchEntries();
      } else {
        toast.error(data.error || "保存に失敗しました");
      }
    } catch (err) {
      toast.error((err as Error).message || "保存に失敗しました");
    } finally {
      setSaving(false);
    }
  };

  // 招待メール送信
  const handleInvite = async () => {
    if (!selectedEngineer) return;
    const email = window.prompt("エンジニアのメールアドレスを入力してください（Sophiaから招待URLが届きます）");
    if (!email || !email.includes("@")) {
      if (email) toast.error("有効なメールアドレスを入力してください");
      return;
    }

    setInviting(true);
    try {
      const data = await invitePortalEngineer({ engineer_id: selectedEngineer, email });
      if (data.success) {
        toast.success(data.message);
      } else {
        toast.error(data.error || "招待メールの送信に失敗しました");
      }
    } catch (err) {
      toast.error((err as Error).message || "ネットワークエラーが発生しました");
    } finally {
      setInviting(false);
    }
  };

  // ダーティ件数
  const dirtyCount = useMemo(() => days.filter((d) => d.dirty).length, [days]);

  // 月選択オプション（過去6ヶ月 + 翌月）
  const monthOptions = useMemo(() => {
    const options: { year: number; month: number; label: string }[] = [];
    for (let i = -1; i <= 6; i++) {
      const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
      options.push({
        year: d.getFullYear(),
        month: d.getMonth() + 1,
        label: `${d.getFullYear()}年${d.getMonth() + 1}月`,
      });
    }
    return options;
  }, []);

  if (loading && engineers.length === 0) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full" />
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* ヘッダー */}
      <div className="sticky top-0 z-10 bg-card border-b border-border px-6 py-4">
        <div className="max-w-6xl mx-auto flex items-center justify-between flex-wrap gap-4">
          <div className="flex items-center gap-4">
            {engineers.length > 1 && (
              <a href="/portal" className="text-muted-foreground hover:text-foreground transition-colors text-sm">
                ← ポータル
              </a>
            )}
            <h1 className="text-lg font-bold">📝 稼働報告入力</h1>
          </div>
          <div className="flex items-center gap-3">
            {/* エンジニア選択 */}
            {engineers.length > 1 ? (
              <div className="flex items-center gap-2">
                <select
                  className="bg-muted border border-border rounded-lg px-3 py-2 text-sm text-foreground"
                  value={selectedEngineer ?? ""}
                  onChange={(e) => setSelectedEngineer(Number(e.target.value))}
                >
                  {engineers.map((eng) => (
                    <option key={eng.id} value={eng.id}>{eng.name}</option>
                  ))}
                </select>
                <button
                  onClick={handleInvite}
                  disabled={inviting}
                  className="bg-primary/10 text-primary hover:bg-primary/20 text-xs font-medium px-3 py-2 rounded-lg border border-primary/20 transition-colors disabled:opacity-50"
                >
                  {inviting ? "送信中..." : "✉️ 招待"}
                </button>
              </div>
            ) : (
              engineers.length === 1 && (
                <span className="text-sm font-medium px-3 py-2 bg-muted rounded-lg border border-border">
                  👤 {engineers[0].name}
                </span>
              )
            )}
            {/* 月選択 */}
            <select
              className="bg-muted border border-border rounded-lg px-3 py-2 text-sm text-foreground"
              value={`${year}-${month}`}
              onChange={(e) => {
                const [y, m] = e.target.value.split("-").map(Number);
                setYear(y);
                setMonth(m);
              }}
            >
              {monthOptions.map((opt) => (
                <option key={`${opt.year}-${opt.month}`} value={`${opt.year}-${opt.month}`}>
                  {opt.label}
                </option>
              ))}
            </select>
            {/* 保存ボタン */}
            <button
              onClick={handleSave}
              disabled={saving || locked || dirtyCount === 0}
              className="px-4 py-2 bg-primary hover:bg-primary/80 disabled:opacity-40 text-primary-foreground text-sm font-medium rounded-lg transition-colors flex items-center gap-2"
            >
              {saving ? (
                <><span className="animate-spin">⏳</span> 保存中...</>
              ) : (
                <>{dirtyCount > 0 ? `💾 ${dirtyCount}件を保存` : "💾 保存"}</>
              )}
            </button>
          </div>
        </div>
      </div>

      <div className="max-w-6xl mx-auto px-6 py-6 space-y-6">
        {/* ロック警告 */}
        {locked && (
          <div className="bg-amber-500/10 border border-amber-500/30 text-amber-400 rounded-lg px-4 py-3 text-sm flex items-center gap-2">
            🔒 このデータは管理者によって確定済みです。変更できません。
          </div>
        )}

        {/* サマリーカード */}
        {summary && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <SummaryCard label="合計稼働時間" value={summary.total_display} icon="⏱️" />
            <SummaryCard label="稼働日数" value={`${summary.work_days}日`} icon="📅" />
            <SummaryCard label="未入力日数" value={`${summary.missing_count}日`} icon="⚠️"
              variant={summary.missing_count > 0 ? "warning" : "default"} />
            <SummaryCard label="小数表示" value={`${summary.total_hours}h`} icon="🔢" />
          </div>
        )}

        {/* 日次入力テーブル */}
        <div className="bg-card border border-border rounded-lg overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border bg-muted/50">
                  <th className="px-3 py-3 text-left font-medium text-muted-foreground w-24">日付</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-16 whitespace-nowrap">状態</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-28">開始</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-28">終了</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-20">休憩</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-20">実働</th>
                  <th className="px-3 py-3 text-left font-medium text-muted-foreground">作業内容</th>
                  <th className="px-3 py-3 text-center font-medium text-muted-foreground w-16">休日</th>
                </tr>
              </thead>
              <tbody>
                {days.map((day, idx) => {
                  const isMissing = !day.saved && !day.isWeekend && !day.is_holiday &&
                    day.date <= new Date().toISOString().split("T")[0];
                  return (
                    <tr
                      key={day.date}
                      className={`border-b border-border/50 transition-colors ${
                        day.isWeekend ? "bg-muted/20" :
                        day.is_holiday ? "bg-blue-500/5" :
                        day.dirty ? "bg-primary/5" :
                        ""
                      } hover:bg-accent/30`}
                    >
                      {/* 日付 */}
                      <td className="px-3 py-2">
                        <div className="flex items-center gap-2">
                          <span className={`text-xs w-6 text-center rounded py-0.5 ${
                            day.dayOfWeek === 0 ? "text-red-400 bg-red-500/10" :
                            day.dayOfWeek === 6 ? "text-blue-400 bg-blue-500/10" :
                            "text-muted-foreground"
                          }`}>
                            {day.dayLabel}
                          </span>
                          <span className="font-mono tabular-nums">
                            {parseInt(day.date.split("-")[2])}
                          </span>
                        </div>
                      </td>

                      {/* 状態バッジ */}
                      <td className="px-3 py-2 text-center whitespace-nowrap">
                        {day.is_holiday ? (
                          <span className="text-xs px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-400">休日</span>
                        ) : day.saved ? (
                          <span className="text-xs px-1.5 py-0.5 rounded bg-emerald-500/10 text-emerald-400">✓</span>
                        ) : day.isWeekend ? (
                          <span className="text-xs text-muted-foreground">—</span>
                        ) : isMissing ? (
                          <span className="text-xs px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400">未入力</span>
                        ) : null}
                      </td>

                      {/* 開始時間 */}
                      <td className="px-3 py-2 text-center">
                        <TimeSelect
                          value={day.start_time}
                          onChange={(v) => updateDay(idx, "start_time", v)}
                          disabled={locked || day.is_holiday}
                        />
                      </td>

                      {/* 終了時間 */}
                      <td className="px-3 py-2 text-center">
                        <TimeSelect
                          value={day.end_time}
                          onChange={(v) => updateDay(idx, "end_time", v)}
                          disabled={locked || day.is_holiday}
                        />
                      </td>

                      {/* 休憩時間 */}
                      <td className="px-3 py-2 text-center">
                        <select
                          value={day.break_minutes}
                          onChange={(e) => updateDay(idx, "break_minutes", Number(e.target.value))}
                          disabled={locked || day.is_holiday}
                          className="bg-muted border border-border rounded px-1 py-1 text-sm text-foreground text-center disabled:opacity-30"
                        >
                          <option value={0}>0分</option>
                          <option value={30}>30分</option>
                          <option value={45}>45分</option>
                          <option value={60}>60分</option>
                          <option value={90}>90分</option>
                        </select>
                      </td>

                      {/* 実働時間 */}
                      <td className="px-3 py-2 text-center font-mono tabular-nums text-sm">
                        {day.actual_minutes > 0 ? (
                          <span className="text-foreground">{formatMinutes(day.actual_minutes)}</span>
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </td>

                      {/* 作業内容 */}
                      <td className="px-3 py-2">
                        <input
                          type="text"
                          value={day.work_description}
                          onChange={(e) => updateDay(idx, "work_description", e.target.value)}
                          disabled={locked || day.is_holiday}
                          placeholder={day.isWeekend ? "" : "作業内容を入力..."}
                          className="bg-muted border border-border rounded px-2 py-1 text-sm text-foreground w-full placeholder:text-muted-foreground/50 disabled:opacity-30"
                        />
                      </td>

                      {/* 休日チェック */}
                      <td className="px-3 py-2 text-center">
                        <input
                          type="checkbox"
                          checked={day.is_holiday}
                          onChange={(e) => updateDay(idx, "is_holiday", e.target.checked)}
                          disabled={locked}
                          className="w-4 h-4 accent-primary cursor-pointer disabled:opacity-30"
                        />
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          {/* フッター: 合計 */}
          {days.length > 0 && (
            <div className="px-4 py-3 border-t border-border bg-muted/30 flex items-center justify-between text-sm">
              <span className="text-muted-foreground">
                入力済: {days.filter((d) => d.saved || (d.start_time && d.end_time)).length}日 /
                稼働日: {days.filter((d) => !d.isWeekend && !d.is_holiday).length}日
              </span>
              <span className="font-semibold">
                合計: {formatMinutes(days.reduce((sum, d) => sum + d.actual_minutes, 0))}
              </span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── サマリーカード ──

function SummaryCard({ label, value, icon, variant = "default" }: {
  label: string;
  value: string;
  icon: string;
  variant?: "default" | "warning";
}) {
  return (
    <div className={`bg-card border rounded-lg px-4 py-3 ${
      variant === "warning" ? "border-amber-500/30" : "border-border"
    }`}>
      <div className="flex items-center gap-2 text-foreground/70 text-xs mb-1">
        <span>{icon}</span>
        <span>{label}</span>
      </div>
      <div className={`text-xl font-bold tabular-nums ${
        variant === "warning" ? "text-amber-400" : "text-foreground"
      }`}>
        {value}
      </div>
    </div>
  );
}
