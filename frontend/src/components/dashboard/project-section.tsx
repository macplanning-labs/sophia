// components/dashboard/project-section.tsx — ホームダッシュボード「プロジェクト別」セクション
//
// PC = テーブル、モバイル = カード(Tailwindブレークポイントで出し分け)。
// 行/カードクリックで詳細パネルをインライン展開する。
// 発注進捗・受注進捗の行クリックで注文書詳細モーダル(DocumentDetailModal)を開く。
// 案件展開パネルから「＋発注書作成」「＋受注書作成」で当月分を一括起票できる。

"use client";

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { toast } from "sonner";
import { fetchProjectDashboard, fetchProjects, fetchProjectDetail, createOrdersFromContracts, createReceivedOrderFromContract } from "@/lib/api";
import type { ProjectSummary, ProgressRow, ProjectRow } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { PipelineTable, PO_DISPLAY_STEPS, RO_DISPLAY_STEPS } from "@/components/dashboard/pipeline-table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { DocumentDetailModal, type DocumentModalTarget } from "@/components/dashboard/document-detail-modal";

function formatYen(n: number): string {
  return `¥${n < 0 ? "-" : ""}${Math.abs(n).toLocaleString("ja-JP")}`;
}

function workerNames(p: ProjectSummary): string {
  const names = p.engineers.map((e) => e.row.engineer_name);
  return names.length > 0 ? names.join("、") : "—";
}

/** 過去pastCountヶ月〜未来futureCountヶ月を、未来→過去の順で列挙する。
 * 案件展開パネルの「＋発注書作成」等で未来月分を先に起票できるため、
 * ダッシュボードの対象年月フィルタにも未来月を含める必要がある。 */
function recentMonths(pastCount: number, futureCount = 0): { value: string; label: string }[] {
  const now = new Date();
  const result: { value: string; label: string }[] = [];
  for (let i = futureCount; i >= -(pastCount - 1); i--) {
    const d = new Date(now.getFullYear(), now.getMonth() + i, 1);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    // 同じ書式の項目が並ぶと誤クリックしやすいため、当月だけ明示する
    const label = i === 0 ? `${yyyy}年${mm}月（今月）` : `${yyyy}年${mm}月`;
    result.push({ value: `${yyyy}-${mm}-01`, label });
  }
  return result;
}

/** "2026-08-01" 形式の当月値（対象年月の既定選択・作成ダイアログの既定月に使う） */
function currentMonthValue(): string {
  const now = new Date();
  const yyyy = now.getFullYear();
  const mm = String(now.getMonth() + 1).padStart(2, "0");
  return `${yyyy}-${mm}-01`;
}

/** "2025-08-01" / "2025-08" / "2025/08" → "2025-08" */
function monthKey(m: string): string {
  return m.replace(/\//g, "-").slice(0, 7);
}

function emptyProjectSummary(p: ProjectRow): ProjectSummary {
  return {
    project_id: p.project_id,
    project_name: p.name,
    client_name: p.client_name,
    staff_internal_count: 0,
    staff_partner_count: 0,
    timesheet_approved_count: 0,
    timesheet_total_count: 0,
    pending_engineer_names: [],
    billing_total: 0,
    partner_cost_total: 0,
    internal_cost_total: 0,
    profit: 0,
    is_finalized: false,
    engineers: [],
  };
}

function StatusBadge({ isFinalized }: { isFinalized: boolean }) {
  return isFinalized ? (
    <Badge variant="outline" className="bg-emerald-500/15 text-emerald-400 border-emerald-500/30">確定</Badge>
  ) : (
    // default の bg-primary に薄い字が乗ると読めないため、一括取込と同様 text-primary-foreground
    <Badge className="bg-primary text-primary-foreground border-transparent">予定</Badge>
  );
}

function buildWorkPeriod(yearMonth: string, contractStart: string, contractEnd: string) {
  const [y, m] = yearMonth.split("-").map(Number);
  const targetMonth = `${yearMonth}-01`;
  const lastDay = new Date(y, m, 0).getDate();
  const monthEnd = `${yearMonth}-${String(lastDay).padStart(2, "0")}`;
  const workStart = contractStart > targetMonth ? contractStart : targetMonth;
  const workEnd = contractEnd < monthEnd ? contractEnd : monthEnd;
  return { target_month: targetMonth, work_start: workStart, work_end: workEnd };
}

/** 指定年月(YYYY-MM)がstart_date〜end_date(YYYY-MM-DD)の契約期間内に含まれるか */
function contractCoversMonth(yearMonth: string, startDate: string, endDate: string): boolean {
  const monthStart = `${yearMonth}-01`;
  const [y, m] = yearMonth.split("-").map(Number);
  const lastDay = new Date(y, m, 0).getDate();
  const monthEndStr = `${yearMonth}-${String(lastDay).padStart(2, "0")}`;
  return startDate <= monthEndStr && endDate >= monthStart;
}

export function ProjectSection({
  partnerProgress = [],
  clientProgress = [],
  progressLoading = false,
}: {
  partnerProgress?: ProgressRow[];
  clientProgress?: ProgressRow[];
  progressLoading?: boolean;
} = {}) {
  const monthOptions = useMemo(() => recentMonths(6, 6), []);
  // 既定は「すべて」: 特定の対象月に紐付けず、進行中/開始予定の案件を一覧できるようにする
  const [month, setMonth] = useState<string>("");
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [clientFilter, setClientFilter] = useState("");
  const [oosMessage, setOosMessage] = useState<string | null>(null);
  // 既定は非表示: 入金(受注側PAID)・支払(発注側PAID)とも完了済みのプロジェクトは
  // 対応の必要がないため一覧から除く。過去の実績を確認したい場合のみチェックで表示する。
  const [showCompleted, setShowCompleted] = useState(false);

  const qc = useQueryClient();
  const [createOrderMonth, setCreateOrderMonth] = useState<string>("");
  const [creatingKind, setCreatingKind] = useState<"purchase" | "received" | null>(null);
  const [docModalTarget, setDocModalTarget] = useState<DocumentModalTarget | null>(null);

  const { data, isLoading: monthLoading } = useQuery({
    queryKey: ["project-dashboard", month],
    queryFn: () => fetchProjectDashboard(month),
    enabled: !!month,
  });
  const { data: allProjects = [], isLoading: projectsLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: fetchProjects,
  });
  const isLoading = month ? monthLoading : projectsLoading;

  const rowsAll: ProjectSummary[] = useMemo(() => {
    if (month) return data ?? [];
    // 「すべて」（既定表示）: 進行中/開始予定の有効な案件全件（月次金額は集計対象外のため 0）
    return allProjects.filter((p) => p.is_active).map(emptyProjectSummary);
  }, [month, data, allProjects]);
  const monthLabel = month
    ? (monthOptions.find((m) => m.value === month)?.label ?? month)
    : "すべて";
  const selectedMonthKey = month ? monthKey(month) : "";

  const filterProgressByProjectAndMonth = (progress: ProgressRow[], projectId: string) =>
    progress
      .filter((r) => {
        if (r.project_id !== projectId) return false;
        // 上部の対象年月に合わせる（「すべて」のときは月フィルタなし）
        if (selectedMonthKey && monthKey(r.month) !== selectedMonthKey) return false;
        return true;
      })
      .sort((a, b) => monthKey(b.month).localeCompare(monthKey(a.month)));

  /** 入金(受注側)・支払(発注側)とも完了(PAID)しているプロジェクトか。
   *  対象年月に該当する発注・受注進捗行が1件も無い（＝まだ実績が無い）場合は
   *  「完了」ではなく通常表示のままにする（進行中/未着手と紛れないようにするため）。 */
  const isProjectComplete = (projectId: string) => {
    const po = filterProgressByProjectAndMonth(partnerProgress, projectId);
    const ro = filterProgressByProjectAndMonth(clientProgress, projectId);
    if (po.length === 0 && ro.length === 0) return false;
    const poComplete = po.length === 0 || po.every((r) => r.status_value === "PAID");
    const roComplete = ro.length === 0 || ro.every((r) => r.status_value === "PAID");
    return poComplete && roComplete;
  };

  const rowsByClient = useMemo(
    () => (clientFilter ? rowsAll.filter((row) => row.client_name === clientFilter) : rowsAll),
    [rowsAll, clientFilter]
  );
  const completedCount = useMemo(
    () => rowsByClient.filter((row) => isProjectComplete(row.project_id)).length,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rowsByClient, partnerProgress, clientProgress, selectedMonthKey]
  );
  const rows = useMemo(
    () => (showCompleted ? rowsByClient : rowsByClient.filter((row) => !isProjectComplete(row.project_id))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rowsByClient, showCompleted, partnerProgress, clientProgress, selectedMonthKey]
  );
  const selected = rows.find((p) => p.project_id === selectedProjectId) ?? null;

  const projectOptions = useMemo(
    () =>
      allProjects.map((p: ProjectRow) => ({
        value: p.project_id,
        label: p.name,
        searchText: p.client_name,
        muted: !rowsAll.some((r) => r.project_id === p.project_id),
      })),
    [allProjects, rowsAll]
  );

  /** 選択肢は対象月のデータに存在するクライアントのみ（他月の残フィルタを防ぐ） */
  const clientOptions = useMemo(() => {
    const names = new Set(rowsAll.map((r) => r.client_name).filter(Boolean));
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [rowsAll]);

  const monthSelectOptions = useMemo(
    () => [
      { value: "", label: "すべて" },
      ...monthOptions.map((m) => ({ value: m.value, label: m.label })),
    ],
    [monthOptions]
  );

  const changeMonth = (next: string) => {
    if (next === month) return;
    setMonth(next);
    // 対象月変更時はクライアントフィルタをクリア（前月の選択が空結果を生むのを防ぐ）
    setClientFilter("");
    setOosMessage(null);
    // 選択中の案件は維持する。新月の rows に含まれなければ selected が消え、
    // 含まれる場合は発注・受注進捗が selectedMonthKey で同月に絞り込まれる。
  };

  const pickProject = (projectId: string) => {
    if (!projectId) {
      setSelectedProjectId(null);
      setOosMessage(null);
      return;
    }
    const p = allProjects.find((x) => x.project_id === projectId);
    const inScope = rowsAll.some((r) => r.project_id === projectId);
    if (inScope) {
      // クライアントフィルタで隠れていれば外す
      const row = rowsAll.find((r) => r.project_id === projectId);
      if (row && clientFilter && row.client_name !== clientFilter) {
        setClientFilter("");
      }
      setSelectedProjectId(projectId);
      setOosMessage(null);
    } else {
      setOosMessage(
        `『${p?.name ?? projectId}』は${monthLabel}に稼働実績がありません。対象月を変更するか、案件ページから直接確認してください。`
      );
    }
  };

  const createOrdersMutation = useMutation({
    mutationFn: async ({ projectId, yearMonth }: { projectId: string; yearMonth: string }) => {
      const detail = await fetchProjectDetail(projectId);
      const partnerContracts: { id: number; start_date: string; end_date: string; is_active: boolean }[] =
        detail?.partner_contracts ?? [];
      const targetIds = partnerContracts
        .filter((c) => c.is_active && contractCoversMonth(yearMonth, c.start_date, c.end_date))
        .map((c) => c.id);
      if (targetIds.length === 0) {
        throw new Error("対象年月に有効な発注契約がありません");
      }
      const [yy, mm] = yearMonth.split("-").map(Number);
      const lastDay = new Date(yy, mm, 0).getDate();
      const work_start = `${yearMonth}-01`;
      const work_end = `${yearMonth}-${String(lastDay).padStart(2, "0")}`;
      const res = await createOrdersFromContracts({ contract_ids: targetIds, work_start, work_end });
      if (res.success === false) throw new Error(res.error ?? "発注書の作成に失敗しました");
      return res;
    },
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["project-dashboard"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      const created = res.order_ids?.length ?? 0;
      const skipped = res.skipped_partners?.length ?? 0;
      toast.success(`発注書を${created}件作成しました${skipped > 0 ? `（${skipped}件は既存のためスキップ）` : ""}`);
      setCreatingKind(null);
    },
    onError: (e: Error) => toast.error(`発注書作成エラー: ${e.message}`),
  });

  const createReceivedOrdersMutation = useMutation({
    mutationFn: async ({ projectId, yearMonth }: { projectId: string; yearMonth: string }) => {
      const detail = await fetchProjectDetail(projectId);
      const clientContracts: { id: number; start_date: string; end_date: string; is_active: boolean }[] =
        detail?.client_contracts ?? [];
      const targets = clientContracts.filter(
        (c) => c.is_active && contractCoversMonth(yearMonth, c.start_date, c.end_date)
      );
      if (targets.length === 0) {
        throw new Error("対象年月に有効な受注契約がありません");
      }
      const results = await Promise.allSettled(
        targets.map((c) => {
          const period = buildWorkPeriod(yearMonth, c.start_date, c.end_date);
          return createReceivedOrderFromContract({ client_contract_id: c.id, ...period });
        })
      );
      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.length - succeeded;
      if (succeeded === 0) {
        throw new Error("受注書を作成できませんでした（既に存在するか対象契約がありません）");
      }
      return { succeeded, failed };
    },
    onSuccess: ({ succeeded, failed }) => {
      qc.invalidateQueries({ queryKey: ["project-dashboard"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success(`受注書を${succeeded}件作成しました${failed > 0 ? `（${failed}件は既存のためスキップ）` : ""}`);
      setCreatingKind(null);
    },
    onError: (e: Error) => toast.error(`受注書作成エラー: ${e.message}`),
  });

  const openFromPoProgress = (row: ProgressRow) => {
    if (!row.order_id) {
      toast.error("この行に発注書がありません");
      return;
    }
    setDocModalTarget({
      kind: "purchase",
      engineerName: row.engineer_name,
      orderId: row.order_id,
      progressMonth: row.month,
    });
  };

  const openFromRoProgress = (row: ProgressRow) => {
    if (!row.order_id) {
      toast.error("この行に受注書がありません");
      return;
    }
    setDocModalTarget({
      kind: "received",
      engineerName: row.engineer_name,
      orderId: row.order_id,
      progressMonth: row.month,
    });
  };

  if (isLoading) {
    return (
      <Card className="bg-card border-border">
        <CardHeader className="pb-2">
          <CardTitle className="text-base">🗂 プロジェクト別</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {[...Array(3)].map((_, i) => (
              <div key={i} className="h-12 bg-muted/50 rounded animate-pulse" />
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="bg-card border-border">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between flex-wrap gap-2">
          <div>
            <CardTitle className="text-base">🗂 プロジェクト別</CardTitle>
            <p className="text-xs text-muted-foreground">{rows.length}件</p>
          </div>
          <div className="flex items-center gap-3 flex-wrap">
            <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
              対象年月:
              <select
                value={month}
                onChange={(e) => changeMonth(e.target.value)}
                className="bg-background border border-border rounded px-2 py-1 text-xs text-foreground"
              >
                {monthSelectOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer">
              <input
                type="checkbox"
                checked={showCompleted}
                onChange={(e) => setShowCompleted(e.target.checked)}
              />
              完了済みも表示{completedCount > 0 ? `（${completedCount}件）` : ""}
            </label>
          </div>
        </div>
      </CardHeader>

      <CardContent>
        {oosMessage && (
          <div className="mb-3 flex items-center justify-between gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-400">
            <span>{oosMessage}</span>
            <button onClick={() => setOosMessage(null)} className="font-bold hover:opacity-70">✕</button>
          </div>
        )}
        {rows.length === 0 ? (
          <div className="py-4 text-center space-y-2">
            <p className="text-sm text-muted-foreground">
              {clientFilter
                ? `「${clientFilter}」に一致するプロジェクトがありません`
                : "該当するデータがありません"}
            </p>
            {clientFilter && (
              <button
                type="button"
                className="text-xs text-sky-400 hover:underline"
                onClick={() => setClientFilter("")}
              >
                クライアントフィルタをすべてに戻す
              </button>
            )}
            <div className="mt-2 flex items-center justify-center gap-3 flex-wrap">
              <SearchableColumnHeader
                label="プロジェクト名"
                options={projectOptions}
                value=""
                onChange={pickProject}
                placeholder="案件名で検索…"
              />
              <SearchableColumnHeader
                label="対象年月"
                options={monthSelectOptions}
                value={month}
                onChange={changeMonth}
                allowClear={false}
                placeholder="年月で検索…"
              />
            </div>
          </div>
        ) : (
          <>
            <div className="hidden md:block overflow-x-auto">
              <table className="w-full text-xs border-collapse">
                <thead>
                  <tr className="border-b border-border">
                    <th className="text-left py-1.5 px-2">
                      <SearchableColumnHeader
                        label="プロジェクト名"
                        options={projectOptions}
                        value=""
                        onChange={pickProject}
                        placeholder="案件名で検索…"
                      />
                    </th>
                    <th className="text-left py-1.5 px-2">
                      <SearchableColumnHeader
                        label="クライアント"
                        options={clientOptions}
                        value={clientFilter}
                        onChange={(v) => {
                          setClientFilter(v);
                          setSelectedProjectId(null);
                        }}
                        placeholder="クライアント名で検索…"
                      />
                    </th>
                    <th className="text-left py-1.5 px-2">
                      <SearchableColumnHeader
                        label="対象年月"
                        options={monthSelectOptions}
                        value={month}
                        onChange={changeMonth}
                        allowClear={false}
                        placeholder="年月で検索…"
                      />
                    </th>
                    <th className="text-left py-1.5 px-2 text-muted-foreground font-medium">作業者</th>
                    <th className="text-center py-1.5 px-2 text-muted-foreground font-medium">稼働報告</th>
                    <th className="text-right py-1.5 px-2 text-muted-foreground font-medium">売上合計</th>
                    <th className="text-right py-1.5 px-2 text-muted-foreground font-medium">支払合計</th>
                    <th className="text-right py-1.5 px-2 text-muted-foreground font-medium">当月利益</th>
                    <th className="text-center py-1.5 px-2 text-muted-foreground font-medium">状態</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((p) => (
                    <tr
                      key={p.project_id}
                      onClick={() => setSelectedProjectId(selectedProjectId === p.project_id ? null : p.project_id)}
                      className={cn(
                        "border-b border-border/40 cursor-pointer transition-colors hover:bg-muted/40",
                        selectedProjectId === p.project_id && "ring-1 ring-indigo-500/50"
                      )}
                    >
                      <td className="py-1.5 px-2 font-medium text-foreground">{p.project_name}</td>
                      <td className="py-1.5 px-2 text-muted-foreground">{p.client_name}</td>
                      <td className="py-1.5 px-2 text-muted-foreground whitespace-nowrap">{monthLabel}</td>
                      <td className="py-1.5 px-2 text-muted-foreground">{workerNames(p)}</td>
                      <td className="py-1.5 px-2 text-center text-muted-foreground">
                        {p.timesheet_approved_count}/{p.timesheet_total_count}
                      </td>
                      <td className="py-1.5 px-2 text-right text-foreground">{formatYen(p.billing_total)}</td>
                      <td className="py-1.5 px-2 text-right text-muted-foreground">{formatYen(p.partner_cost_total)}</td>
                      <td className="py-1.5 px-2 text-right font-medium text-emerald-400">{formatYen(p.profit)}</td>
                      <td className="py-1.5 px-2 text-center">
                        <StatusBadge isFinalized={p.is_finalized} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <div className="md:hidden space-y-2">
              <div className="flex gap-2 mb-2 flex-wrap">
                <SearchableColumnHeader
                  label="クライアント"
                  options={clientOptions}
                  value={clientFilter}
                  onChange={(v) => {
                    setClientFilter(v);
                    setSelectedProjectId(null);
                  }}
                  placeholder="クライアント名で検索…"
                />
                <SearchableColumnHeader
                  label="対象年月"
                  options={monthSelectOptions}
                  value={month}
                  onChange={changeMonth}
                  allowClear={false}
                  placeholder="年月で検索…"
                />
              </div>
              {rows.map((p) => (
                <div
                  key={p.project_id}
                  onClick={() => setSelectedProjectId(selectedProjectId === p.project_id ? null : p.project_id)}
                  className={cn(
                    "rounded-lg border border-border bg-muted/20 p-3 cursor-pointer",
                    selectedProjectId === p.project_id && "ring-1 ring-indigo-500/50"
                  )}
                >
                  <div className="flex justify-between items-start mb-2">
                    <div>
                      <div className="font-medium text-sm text-foreground">{p.project_name}</div>
                      <div className="text-xs text-muted-foreground">{p.client_name} · {monthLabel}</div>
                    </div>
                    <StatusBadge isFinalized={p.is_finalized} />
                  </div>
                  <div className="flex gap-3 text-xs text-muted-foreground mb-2">
                    <span>作業者: {workerNames(p)}</span>
                    <span>稼働報告 {p.timesheet_approved_count}/{p.timesheet_total_count}</span>
                  </div>
                  <table className="w-full text-xs">
                    <tbody>
                      <tr>
                        <td className="text-muted-foreground py-0.5">売上合計</td>
                        <td className="text-right text-foreground">{formatYen(p.billing_total)}</td>
                      </tr>
                      <tr>
                        <td className="text-muted-foreground py-0.5">支払合計</td>
                        <td className="text-right text-foreground">{formatYen(p.partner_cost_total)}</td>
                      </tr>
                      <tr>
                        <td className="text-muted-foreground py-0.5">自社人件費配分</td>
                        <td className="text-right text-foreground">{formatYen(p.internal_cost_total)}</td>
                      </tr>
                      <tr className="border-t border-border/50">
                        <td className="font-medium text-foreground pt-1">当月利益</td>
                        <td className="text-right font-medium text-emerald-400 pt-1">{formatYen(p.profit)}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              ))}
            </div>

            {selected && (
              <div className="mt-3 p-4 rounded-lg bg-muted/30 border border-border/50">
                <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
                  <div className="text-sm font-semibold text-foreground">
                    {selected.project_name} / {selected.client_name}
                  </div>
                  <div className="flex items-center gap-2">
                    {month && (
                      <Link
                        href={`/settlement?month=${month}&project_id=${selected.project_id}`}
                        className="text-xs border border-sky-500/30 text-sky-400 rounded px-2 py-1"
                      >
                        月次確定を見る ↗
                      </Link>
                    )}
                    <button
                      type="button"
                      onClick={() => { setCreatingKind("purchase"); setCreateOrderMonth((month || currentMonthValue()).slice(0, 7)); }}
                      className="text-xs border border-orange-500/30 text-orange-400 rounded px-2 py-1 hover:bg-orange-500/10"
                    >
                      ＋発注書作成
                    </button>
                    <button
                      type="button"
                      onClick={() => { setCreatingKind("received"); setCreateOrderMonth((month || currentMonthValue()).slice(0, 7)); }}
                      className="text-xs border border-emerald-500/30 text-emerald-400 rounded px-2 py-1 hover:bg-emerald-500/10"
                    >
                      ＋受注書作成
                    </button>
                    <button
                      onClick={() => setSelectedProjectId(null)}
                      className="text-muted-foreground hover:text-foreground text-xs px-2 py-1 rounded bg-muted/40"
                    >
                      ✕ 閉じる
                    </button>
                  </div>
                </div>
                <p className="text-[11px] text-muted-foreground mb-2">
                  発注・受注進捗の作業者行をクリックすると、注文書の詳細モーダルが開きます
                  {selectedMonthKey ? `（対象年月: ${monthLabel}）` : "（全期間）"}
                </p>
                {(() => {
                  const poRows = filterProgressByProjectAndMonth(partnerProgress, selected.project_id);
                  const roRows = filterProgressByProjectAndMonth(clientProgress, selected.project_id);
                  return (
                    <>
                      {poRows.length > 0 && (
                        <div className="mt-2">
                          <PipelineTable
                            title="📦 発注進捗"
                            rows={poRows}
                            isLoading={progressLoading}
                            color="orange"
                            linkPrefix="/orders"
                            linkLabel="注文書を見る"
                            entityColumnLabel="所属会社"
                            stepIndices={PO_DISPLAY_STEPS.indices}
                            stepLabels={PO_DISPLAY_STEPS.labels}
                            reportStepIndex={PO_DISPLAY_STEPS.reportIndex}
                            bare
                            onRowClick={openFromPoProgress}
                          />
                        </div>
                      )}
                      {roRows.length > 0 && (
                        <div className="mt-4">
                          <PipelineTable
                            title="📊 受注進捗"
                            rows={roRows}
                            isLoading={progressLoading}
                            color="emerald"
                            linkPrefix="/received-orders"
                            linkLabel="受注書を見る"
                            stepIndices={RO_DISPLAY_STEPS.indices}
                            stepLabels={RO_DISPLAY_STEPS.labels}
                            reportStepIndex={RO_DISPLAY_STEPS.reportIndex}
                            bare
                            onRowClick={openFromRoProgress}
                          />
                        </div>
                      )}
                      {poRows.length === 0 && roRows.length === 0 && (
                        <p className="text-xs text-muted-foreground py-4 text-center">
                          {selectedMonthKey
                            ? `${monthLabel}の発注・受注進捗はありません`
                            : "発注・受注進捗はありません"}
                        </p>
                      )}
                    </>
                  );
                })()}
              </div>
            )}
          </>
        )}

        {creatingKind && selected && (
          <div
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
            onClick={(e) => { if (e.target === e.currentTarget) setCreatingKind(null); }}
          >
            <div className="bg-card border border-border rounded-xl shadow-2xl w-full max-w-sm mx-4 p-5">
              <h3 className="text-sm font-semibold text-foreground mb-3">
                {creatingKind === "purchase" ? "発注書作成" : "受注書作成"} — {selected.project_name}
              </h3>
              <label className="block text-xs text-muted-foreground mb-1">対象年月</label>
              <input
                type="month"
                value={createOrderMonth}
                onChange={(e) => setCreateOrderMonth(e.target.value)}
                className="w-full bg-muted border border-border rounded-md px-3 py-2 text-sm text-foreground mb-4"
              />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setCreatingKind(null)}
                  className="text-xs px-3 py-1.5 rounded-md bg-muted/40 text-muted-foreground"
                >
                  キャンセル
                </button>
                <button
                  type="button"
                  disabled={!createOrderMonth || createOrdersMutation.isPending || createReceivedOrdersMutation.isPending}
                  onClick={() => {
                    if (!selected || !createOrderMonth) return;
                    if (creatingKind === "purchase") {
                      createOrdersMutation.mutate({ projectId: selected.project_id, yearMonth: createOrderMonth });
                    } else {
                      createReceivedOrdersMutation.mutate({ projectId: selected.project_id, yearMonth: createOrderMonth });
                    }
                  }}
                  className="text-xs px-3 py-1.5 rounded-md bg-primary text-primary-foreground disabled:opacity-50"
                >
                  作成する
                </button>
              </div>
            </div>
          </div>
        )}
      </CardContent>

      {docModalTarget && (
        <DocumentDetailModal
          open={docModalTarget != null}
          onClose={() => {
            setDocModalTarget(null);
            qc.invalidateQueries({ queryKey: ["project-dashboard"] });
            qc.invalidateQueries({ queryKey: ["dashboard"] });
          }}
          target={docModalTarget}
          projectName={selected?.project_name ?? ""}
          clientName={selected?.client_name ?? ""}
          partnerProgress={partnerProgress}
          clientProgress={clientProgress}
        />
      )}
    </Card>
  );
}
