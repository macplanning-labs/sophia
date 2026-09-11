// components/dashboard/pipeline-table.tsx — 発注進捗/受注進捗のパイプラインテーブル
//
// bare=true: プロジェクト詳細パネル向け(フィルタなし・凝縮列)
// showFilters=true: /orders・/received-orders 向け正式管理画面(パートナー/月/未完了フィルタ)

"use client";

import { useMemo, useState, type ReactNode } from "react";
import Link from "next/link";
import type { ProgressRow } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

export type PipelineColor = "orange" | "emerald";
export type PipelineFilterStatus = "" | "pending" | "overdue" | "active" | "done";

/** row.steps(BEの生ステップ配列)のうち、どのindexを何ラベルで表示するか。両テーブルで列数を5に揃えるため、
 *  indices.length < 5 の残りは空欄セルで埋める(発注進捗と受注進捗の縦のラインを揃えるため)。 */
export const PO_DISPLAY_STEPS = {
  indices: [1, 2, 4, 5, 6],
  labels: ["発注", "受諾", "支払通知", "受諾", "支払"],
  reportIndex: 3, // "報告"(REPORT_RECEIVED) — 報告書チェック列に単独表示
};
export const RO_DISPLAY_STEPS = {
  indices: [0, 3, 4, 5, 6],
  labels: ["受注作成", "請求作成", "請求送付", "受諾", "入金"],
  reportIndex: 1, // "勤怠"(REPORT_RECEIVED) — 報告書チェック列に単独表示
};
const STEP_SLOTS = 5;

export function StepDotTable({ done, active, color }: { done: boolean; active: boolean; color: PipelineColor }) {
  if (done) {
    return (
      <div className="inline-flex items-center justify-center w-4 h-4 rounded-full bg-emerald-500 text-foreground text-[8px]">✓</div>
    );
  }
  if (active) {
    return (
      <div className={cn(
        "inline-flex items-center justify-center w-4 h-4 rounded-full text-foreground text-[8px] animate-pulse",
        color === "orange" ? "bg-amber-500 shadow-[0_0_4px_rgba(245,158,11,0.4)]" : "bg-emerald-500 shadow-[0_0_4px_rgba(16,185,129,0.4)]"
      )}>●</div>
    );
  }
  return (
    <div className="inline-flex items-center justify-center w-4 h-4 rounded-full border border-border" />
  );
}

export function StepCard({ done, active, label, color }: { done: boolean; active: boolean; label: string; color: PipelineColor }) {
  const statusColor = done ? "#10B981" : active ? "#F59E0B" : "#4B5563";
  const bg = done ? "rgba(16,185,129,0.08)" : active ? "rgba(245,158,11,0.08)" : "rgba(75,85,99,0.06)";

  return (
    <div
      className="flex-1 min-w-[90px] px-3 py-2 rounded-md"
      style={{ background: bg, borderLeft: `3px solid ${statusColor}` }}
    >
      <div className="text-[11px] font-semibold" style={{ color: statusColor }}>{label}</div>
      <div className="text-[10px] text-muted-foreground mt-0.5">
        {done ? "完了" : active ? "進行中" : "未着手"}
      </div>
    </div>
  );
}

export function RowStatusBadge({ status }: { status: string }) {
  switch (status) {
    case "done":
      return <Badge variant="outline" className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[9px] px-1.5">完了</Badge>;
    case "overdue":
      return <Badge variant="outline" className="bg-red-500/20 text-red-400 border-red-500/30 text-[9px] px-1.5">遅延</Badge>;
    case "active":
      return <Badge variant="outline" className="bg-amber-500/20 text-amber-400 border-amber-500/30 text-[9px] px-1.5">進行中</Badge>;
    default:
      return <Badge variant="outline" className="bg-muted/40 text-muted-foreground border-border text-[9px] px-1.5">未着手</Badge>;
  }
}

/** 発注パイプライン行の次アクション(ディープリンク)。WS7。 */
export function orderActionLinks(row: ProgressRow): { href: string; label: string }[] {
  const detail = `/orders/${row.order_id}`;
  switch (row.status_value) {
    case "DRAFT":
      return [{ href: detail, label: "注文書を送付" }];
    case "SENT":
    case "ACCEPTED":
      return [
        { href: detail, label: "詳細・提出依頼" },
      ];
    case "REPORT_RECEIVED":
      return [
        { href: "/settlement", label: "支払通知を発行" },
        { href: detail, label: "注文書を見る" },
      ];
    case "NOTICE_CREATED":
    case "NOTICE_CONFIRMED":
      return [{ href: detail, label: "注文書を見る" }];
    default:
      return [{ href: detail, label: "注文書を見る" }];
  }
}

/** 受注パイプライン行の次アクション(WS8向け・先に用意)。 */
export function receivedOrderActionLinks(row: ProgressRow): { href: string; label: string }[] {
  const detail = `/received-orders/${row.order_id}`;
  switch (row.status_value) {
    case "REGISTERED":
      return [{ href: detail, label: "受注書・稼働確認" }];
    case "REPORT_SENT":
      return [
        { href: "/settlement", label: "請求書を発行" },
        { href: detail, label: "受注書を見る" },
      ];
    case "INVOICED":
      return [{ href: detail, label: "受注書を見る" }];
    default:
      return [{ href: detail, label: "受注書を見る" }];
  }
}

export function PipelineTable({
  title,
  rows,
  isLoading = false,
  color,
  linkPrefix,
  linkLabel = "詳細を見る",
  stepIndices,
  stepLabels,
  reportStepIndex,
  entityColumnLabel,
  bare = false,
  emptyMessage = "該当するデータがありません",
  showFilters = false,
  partnerList = [],
  monthList = [],
  defaultFilterStatus = "pending",
  entityFilterLabel = "全パートナー",
  resolveActions,
  renderExpanded,
  onRowClick,
}: {
  title?: string;
  rows: ProgressRow[];
  isLoading?: boolean;
  color: PipelineColor;
  linkPrefix: string;
  linkLabel?: string;
  stepIndices: number[];
  stepLabels: string[];
  reportStepIndex: number;
  entityColumnLabel?: string;
  bare?: boolean;
  emptyMessage?: string;
  showFilters?: boolean;
  partnerList?: string[];
  monthList?: string[];
  defaultFilterStatus?: PipelineFilterStatus;
  entityFilterLabel?: string;
  /** 展開パネルの追加アクション。未指定時は linkPrefix の詳細リンクのみ。 */
  resolveActions?: (row: ProgressRow) => { href: string; label: string }[];
  /** 展開パネル下部に詳細画面などを埋め込む。close で展開を閉じる。 */
  renderExpanded?: (row: ProgressRow, helpers: { close: () => void }) => ReactNode;
  /** 指定時はインライン展開せず、行クリックでコールバック（ダッシュボードの帳票モーダル用） */
  onRowClick?: (row: ProgressRow) => void;
}) {
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const [filterPartner, setFilterPartner] = useState("");
  const [filterMonth, setFilterMonth] = useState("");
  const [filterStatus, setFilterStatus] = useState<PipelineFilterStatus>(defaultFilterStatus);

  const filtered = useMemo(() => {
    if (!showFilters) return rows;
    return rows.filter((r) => {
      if (filterPartner && r.entity_name !== filterPartner) return false;
      if (filterMonth && r.month !== filterMonth) return false;
      if (filterStatus === "pending" && r.row_status === "done") return false;
      else if (filterStatus && filterStatus !== "pending" && r.row_status !== filterStatus) return false;
      return true;
    });
  }, [rows, showFilters, filterPartner, filterMonth, filterStatus]);

  // ダッシュボード(bare) or onRowClick 指定時はインライン展開しない（モーダルへ委譲）
  const modalOnly = bare || !!onRowClick;
  const selectedRow = !modalOnly && selectedIdx !== null ? filtered[selectedIdx] : null;

  if (isLoading) {
    const skeleton = (
      <div className="space-y-3">
        {[...Array(2)].map((_, i) => (
          <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />
        ))}
      </div>
    );
    if (bare) {
      return (
        <div>
          {title && <div className="text-sm font-semibold text-foreground mb-2">{title}</div>}
          {skeleton}
        </div>
      );
    }
    return (
      <Card className="bg-card border-border">
        <CardHeader><CardTitle className="text-base">{title}</CardTitle></CardHeader>
        <CardContent>{skeleton}</CardContent>
      </Card>
    );
  }

  const filterBar = showFilters ? (
    <div className="flex items-center gap-3 flex-wrap mb-2">
      {partnerList.length > 0 && (
        <select
          value={filterPartner}
          onChange={(e) => { setFilterPartner(e.target.value); setSelectedIdx(null); }}
          className="bg-muted border border-border text-foreground rounded px-2 py-1 text-xs h-7 max-w-[140px]"
        >
          <option value="">{entityFilterLabel}</option>
          {partnerList.map((p) => <option key={p} value={p}>{p}</option>)}
        </select>
      )}
      {monthList.length > 0 && (
        <select
          value={filterMonth}
          onChange={(e) => { setFilterMonth(e.target.value); setSelectedIdx(null); }}
          className="bg-muted border border-border text-foreground rounded px-2 py-1 text-xs h-7 max-w-[100px]"
        >
          <option value="">全期間</option>
          {monthList.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
      )}
      <div className="flex gap-0.5 h-7">
        {([
          { value: "pending" as PipelineFilterStatus, label: "未完了", chip: "text-indigo-400 bg-indigo-500/15" },
          { value: "overdue" as PipelineFilterStatus, label: "遅延", chip: "text-red-400 bg-red-500/15" },
          { value: "active" as PipelineFilterStatus, label: "進行中", chip: "text-amber-400 bg-amber-500/15" },
          { value: "done" as PipelineFilterStatus, label: "完了", chip: "text-emerald-400 bg-emerald-500/15" },
          { value: "" as PipelineFilterStatus, label: "全て", chip: "text-muted-foreground bg-muted/40" },
        ]).map(({ value, label, chip }) => (
          <button
            key={value || "all"}
            type="button"
            onClick={() => { setFilterStatus(value); setSelectedIdx(null); }}
            className={cn(
              "px-2 text-[11px] border-none rounded cursor-pointer transition-all",
              chip,
              filterStatus === value ? "font-bold opacity-100" : "opacity-50 hover:opacity-75"
            )}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="flex gap-2 text-[10px] text-muted-foreground ml-auto items-center">
        <span>🟢完了</span><span>🟡進行中</span><span>🔴遅延</span><span>⚪未着手</span>
      </div>
    </div>
  ) : null;

  const headerPart = (
    <>
      {bare ? (
        title && <span className="text-sm font-semibold text-foreground">{title}</span>
      ) : (
        <div>
          {title && <CardTitle className="text-base">{title}</CardTitle>}
          {filterBar}
        </div>
      )}
      <p className="text-xs text-muted-foreground mt-0.5">{filtered.length}件</p>
    </>
  );

  const actionLinks = selectedRow
    ? (resolveActions?.(selectedRow) ?? [{ href: `${linkPrefix}/${selectedRow.order_id}`, label: linkLabel }])
    : [];

  const contentPart = filtered.length === 0 ? (
    <p className="text-sm text-muted-foreground py-4 text-center">{emptyMessage}</p>
  ) : (
    <>
      <div className="overflow-x-auto">
        <table className="w-full text-xs border-collapse table-fixed">
          <colgroup>
            {/* モック docs/mocks/pipeline_dashboard_mock.html の列幅に合わせる。
                所属会社列はラベル無しでも幅を確保し、発注/受注テーブルの縦線を揃える */}
            <col style={{ width: 96 }} />
            <col style={{ width: 190 }} />
            <col style={{ width: 68 }} />
            {Array.from({ length: STEP_SLOTS }).map((_, i) => <col key={i} style={{ width: 50 }} />)}
            <col style={{ width: 72 }} />
            <col style={{ width: 96 }} />
            <col style={{ width: 56 }} />
          </colgroup>
          <thead>
            <tr className="border-b border-border">
              <th className="text-left py-1.5 px-2 text-muted-foreground font-medium">作業者</th>
              <th className="text-left py-1.5 px-2 text-muted-foreground font-medium overflow-hidden">{entityColumnLabel ?? ""}</th>
              <th className="text-center py-1.5 px-1 text-muted-foreground font-medium">対象年月</th>
              {Array.from({ length: STEP_SLOTS }).map((_, i) => (
                <th key={i} className="text-center py-1.5 px-0 text-muted-foreground font-medium" style={{ fontSize: "0.6rem" }}>
                  {stepLabels[i] ?? ""}
                </th>
              ))}
              <th className="text-center py-1.5 px-1 text-muted-foreground font-medium">状態</th>
              <th className="text-left py-1.5 px-1 text-muted-foreground font-medium">コメント</th>
              <th className="text-center py-1.5 px-1 text-muted-foreground font-medium">報告書</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((row, i) => {
              const reportDone = row.steps[reportStepIndex]?.done ?? false;
              return (
                <tr
                  key={`${row.order_id}-${i}`}
                  onClick={() => {
                    if (onRowClick) {
                      onRowClick(row);
                      return;
                    }
                    if (bare) return; // bare はモーダル前提。インライン展開しない
                    setSelectedIdx(selectedIdx === i ? null : i);
                  }}
                  className={cn(
                    "border-b border-border/40 cursor-pointer transition-colors",
                    row.row_status === "done" ? "bg-emerald-500/[0.02]" :
                    row.row_status === "overdue" ? "bg-red-500/[0.03]" : "",
                    !modalOnly && selectedIdx === i && "ring-1 ring-indigo-500/50",
                    "hover:bg-muted/40"
                  )}
                  style={{ borderLeft: row.row_status === "overdue" ? "3px solid #EF4444" : row.row_status === "done" ? "3px solid #10B981" : "3px solid transparent" }}
                >
                  <td className="py-1.5 px-2 truncate font-medium" title={row.engineer_name}>{row.engineer_name}</td>
                  <td className="py-1.5 px-2 truncate text-muted-foreground" title={entityColumnLabel ? row.entity_name : undefined}>
                    {entityColumnLabel ? row.entity_name : ""}
                  </td>
                  <td className="py-1.5 px-1 text-center text-muted-foreground">{row.month}</td>
                  {Array.from({ length: STEP_SLOTS }).map((_, si) => {
                    const stepIdx = stepIndices[si];
                    const step = stepIdx !== undefined ? row.steps[stepIdx] : undefined;
                    return (
                      <td key={si} className="py-1.5 px-0 text-center">
                        {step && <StepDotTable done={step.done} active={step.active} color={color} />}
                      </td>
                    );
                  })}
                  <td className="py-1.5 px-1 text-center align-middle">
                    <RowStatusBadge status={row.row_status} />
                  </td>
                  <td className="py-1.5 px-1 align-middle">
                    {row.action_text && row.row_status !== "done" ? (
                      <div
                        className={cn(
                          "whitespace-pre-line leading-tight",
                          row.row_status === "overdue" ? "text-red-400" : "text-muted-foreground"
                        )}
                        style={{ fontSize: "0.6rem" }}
                      >
                        {row.action_text}
                      </div>
                    ) : null}
                  </td>
                  <td className="py-1.5 px-1 text-center">
                    <span className={reportDone ? "text-emerald-400" : "text-muted-foreground/50"}>
                      {reportDone ? "☑" : "▢"}
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {selectedRow && (
        <div className="mt-3 p-4 rounded-lg bg-muted/30 border border-border/50">
          {renderExpanded ? (
            <>
              <div className="flex justify-end mb-2">
                <button
                  type="button"
                  onClick={() => setSelectedIdx(null)}
                  className="text-muted-foreground hover:text-foreground text-xs px-2 py-1 rounded bg-muted/40"
                >
                  ✕ 閉じる
                </button>
              </div>
              <div className="max-h-[70vh] overflow-y-auto">
                {renderExpanded(selectedRow, { close: () => setSelectedIdx(null) })}
              </div>
            </>
          ) : (
            <>
              <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
                <div className="text-sm font-semibold text-foreground">
                  {entityColumnLabel ? `${selectedRow.entity_name} / ` : ""}{selectedRow.engineer_name} — {selectedRow.month}
                  {selectedRow.project_name && (
                    <span className="ml-2 text-xs font-normal text-muted-foreground">{selectedRow.project_name}</span>
                  )}
                  {selectedRow.action_text && selectedRow.row_status !== "done" && (
                    <span className={cn(
                      "ml-2 text-xs font-normal whitespace-pre-line",
                      selectedRow.row_status === "overdue" ? "text-red-400" : "text-muted-foreground"
                    )}>
                      （{selectedRow.action_text.replace("\n", " ")}）
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2 flex-wrap">
                  {actionLinks.map((a) => (
                    <Link key={`${a.href}-${a.label}`} href={a.href}>
                      <Button variant="outline" size="sm" className="text-xs border-blue-500/30 text-blue-400 gap-1">
                        📝 {a.label}
                      </Button>
                    </Link>
                  ))}
                  <button
                    type="button"
                    onClick={() => setSelectedIdx(null)}
                    className="text-muted-foreground hover:text-foreground text-xs px-2 py-1 rounded bg-muted/40"
                  >
                    ✕ 閉じる
                  </button>
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                {stepIndices.map((idx, si) => {
                  const step = selectedRow.steps[idx];
                  if (!step) return null;
                  return <StepCard key={si} done={step.done} active={step.active} label={stepLabels[si]} color={color} />;
                })}
              </div>
            </>
          )}
        </div>
      )}
    </>
  );

  if (bare) {
    return (
      <div>
        {headerPart}
        {contentPart}
      </div>
    );
  }

  return (
    <Card className="bg-card border-border">
      <CardHeader className="pb-2">{headerPart}</CardHeader>
      <CardContent>{contentPart}</CardContent>
    </Card>
  );
}
