"use client";

import { useEffect, useMemo, useState } from "react";
import type { CreatedNotice, SettlementViewRow } from "@/lib/types";
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { HelpTooltip } from "@/components/ui/help-tooltip";
import { toast } from "sonner";
import {
  SearchableColumnHeader,
  type SearchableOption,
} from "@/components/ui/searchable-column-header";
import { getUnapprovedTimesheetGuidance } from "@/lib/guidance/settlement";
import {
  getReadinessChecks,
  isExcludedFromIssue,
  isReadyForIssue,
} from "@/lib/guidance/settlement-readiness";
import { CreatedNoticesInline } from "@/components/settlement/created-notices-inline";
import { IssueReadinessChips } from "@/components/settlement/issue-readiness-chips";

export type SettlementViewMode = "billing" | "payment";

const COLUMN_COUNT = 8;

interface Props {
  rows: SettlementViewRow[];
  isLoading: boolean;
  viewMode: SettlementViewMode;
  /** 請求モード: client_contract_id / 支払モード: partner_contract_id */
  selectedIds: number[];
  onSelectionChange: (ids: number[]) => void;
  onIssueInvoices: () => void;
  onIssueNotices: () => void;
  isIssuingInvoices: boolean;
  isIssuingNotices: boolean;
  month: string;
  onMonthChange: (month: string) => void;
  availableMonths: SearchableOption[];
  clientId: string;
  onClientChange: (id: string) => void;
  clients: SearchableOption[];
  partnerId: string;
  onPartnerChange: (id: string) => void;
  partners: SearchableOption[];
  /** 一括発行直後に表示する支払通知（パートナー行の下にインライン表示） */
  createdNotices?: CreatedNotice[];
  onDismissCreatedNotices?: (partnerId: string) => void;
}

interface Group {
  key: string;
  label: string;
  /** 支払ビューでパートナー契約がない自社グループ */
  selectable: boolean;
  rows: SettlementViewRow[];
  totalHours: number;
  totalAmount: number;
  contractIds: number[];
  allIssued: boolean;
  anyIssued: boolean;
  allApproved: boolean;
  /** 一括発行可能な契約数 / 対象契約数 */
  readyCount: number;
  issueTargetCount: number;
}

function formatYen(n: number): string {
  return `¥${Math.abs(n).toLocaleString("ja-JP")}`;
}

function parseHours(h: string | null): number {
  if (!h) return 0;
  const n = Number(h);
  return Number.isFinite(n) ? n : 0;
}

function formatHours(n: number): string {
  if (n === 0) return "—";
  return Number.isInteger(n) ? `${n}h` : `${n.toFixed(2)}h`;
}

function buildGroups(rows: SettlementViewRow[], mode: SettlementViewMode, monthLabel: string): Group[] {
  const map = new Map<string, SettlementViewRow[]>();

  for (const vr of rows) {
    const key =
      mode === "billing"
        ? `client:${vr.row.client_id}`
        : `partner:${vr.row.partner_id || "__inhouse__"}`;
    const list = map.get(key);
    if (list) list.push(vr);
    else map.set(key, [vr]);
  }

  const groups: Group[] = [];
  for (const [key, groupRows] of map) {
    const first = groupRows[0].row;
    const label =
      mode === "billing"
        ? first.client_name
        : first.partner_name || "自社";
    const selectable =
      mode === "billing" ||
      (!!first.partner_id &&
        !groupRows.every((vr) => vr.row.notice_issued));

    const approved = groupRows.filter((vr) => vr.row.timesheet_status === "APPROVED");
    const totalHours = approved.reduce((s, vr) => s + parseHours(vr.row.total_hours), 0);
    const totalAmount = approved.reduce(
      (s, vr) => s + (mode === "billing" ? vr.billing_amount : vr.payment_amount),
      0
    );
    const contractIds =
      mode === "billing"
        ? groupRows.map((vr) => vr.row.client_contract_id)
        : groupRows
            .map((vr) => vr.row.partner_contract_id)
            .filter((id): id is number => id != null);
    const issuedFlags = groupRows.map((vr) =>
      mode === "billing" ? !!vr.row.invoice_issued : !!vr.row.notice_issued
    );

    let readyCount = 0;
    let issueTargetCount = 0;
    for (const vr of groupRows) {
      const checks = getReadinessChecks(vr, mode, monthLabel);
      if (isExcludedFromIssue(checks)) continue;
      issueTargetCount += 1;
      if (isReadyForIssue(checks)) readyCount += 1;
    }

    groups.push({
      key,
      label,
      selectable,
      rows: groupRows,
      totalHours,
      totalAmount,
      contractIds,
      allIssued: issuedFlags.length > 0 && issuedFlags.every(Boolean),
      anyIssued: issuedFlags.some(Boolean),
      allApproved: groupRows.every((vr) => vr.row.timesheet_status === "APPROVED"),
      readyCount,
      issueTargetCount,
    });
  }

  groups.sort((a, b) => a.label.localeCompare(b.label, "ja"));
  return groups;
}

function IssuedBadge({ allIssued, anyIssued }: { allIssued: boolean; anyIssued: boolean }) {
  if (allIssued) {
    return (
      <Badge variant="outline" className="bg-emerald-500/15 text-emerald-400 border-emerald-500/30 text-[0.65rem]">
        ● 発行済
      </Badge>
    );
  }
  if (anyIssued) {
    return (
      <Badge variant="outline" className="bg-amber-500/15 text-amber-400 border-amber-500/30 text-[0.65rem]">
        △ 一部発行
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="border-slate-500/30 text-slate-400 text-[0.65rem]">
      ○ 未発行
    </Badge>
  );
}

function ChildStatusBadge({
  isApproved,
  hasTimesheet,
}: {
  isApproved: boolean;
  hasTimesheet: boolean;
}) {
  if (isApproved) return null;
  if (hasTimesheet) {
    return (
      <Badge variant="outline" className="bg-amber-500/15 text-amber-400 border-amber-500/30 text-[0.6rem] py-0 px-1.5">
        未承認
      </Badge>
    );
  }
  return (
    <Badge variant="outline" className="bg-rose-500/15 text-rose-400 border-rose-500/30 text-[0.6rem] py-0 px-1.5">
      未提出
    </Badge>
  );
}

function formatMonthLabel(month: string | undefined): string {
  if (!month) return "—";
  const m = month.match(/^(\d{4})-(\d{2})/);
  if (!m) return month;
  return `${m[1]}年${m[2]}月`;
}

export function SettlementTable({
  rows,
  isLoading,
  viewMode,
  selectedIds,
  onSelectionChange,
  onIssueInvoices,
  onIssueNotices,
  isIssuingInvoices,
  isIssuingNotices,
  month,
  onMonthChange,
  availableMonths,
  clientId,
  onClientChange,
  clients,
  partnerId,
  onPartnerChange,
  partners,
  createdNotices = [],
  onDismissCreatedNotices,
}: Props) {
  const monthLabel = formatMonthLabel(month);
  const groups = useMemo(() => buildGroups(rows, viewMode, monthLabel), [rows, viewMode, monthLabel]);
  /** null = 全展開（初期）、Set = 明示的に展開中のキー */
  const [expanded, setExpanded] = useState<Set<string> | null>(null);

  const noticesByPartner = useMemo(() => {
    const map = new Map<string, CreatedNotice[]>();
    for (const n of createdNotices) {
      const list = map.get(n.partner_id);
      if (list) list.push(n);
      else map.set(n.partner_id, [n]);
    }
    return map;
  }, [createdNotices]);

  useEffect(() => {
    if (createdNotices.length === 0) return;
    const keysToExpand = new Set<string>();
    for (const g of groups) {
      const pid = g.rows[0]?.row.partner_id;
      if (pid && noticesByPartner.has(pid)) {
        keysToExpand.add(g.key);
      }
    }
    if (keysToExpand.size === 0) return;
    setExpanded((prev) => {
      const current = prev ?? new Set(groups.map((gr) => gr.key));
      for (const k of keysToExpand) current.add(k);
      return new Set(current);
    });
  }, [createdNotices, groups, noticesByPartner]);

  const isExpanded = (key: string) => {
    if (expanded === null) return true;
    return expanded.has(key);
  };

  const toggleExpand = (key: string) => {
    setExpanded((prev) => {
      const current = prev ?? new Set(groups.map((g) => g.key));
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const selectableGroups = groups.filter((g) => g.selectable);
  const allSelectableIds = selectableGroups.flatMap((g) => g.contractIds);
  const allChecked =
    allSelectableIds.length > 0 &&
    allSelectableIds.every((id) => selectedIds.includes(id));

  const toggleAll = () => {
    if (allChecked) onSelectionChange([]);
    else onSelectionChange(allSelectableIds);
  };

  const isGroupChecked = (g: Group) =>
    g.contractIds.length > 0 && g.contractIds.every((id) => selectedIds.includes(id));

  const isGroupIndeterminate = (g: Group) => {
    const n = g.contractIds.filter((id) => selectedIds.includes(id)).length;
    return n > 0 && n < g.contractIds.length;
  };

  const toggleGroup = (g: Group) => {
    if (!g.selectable) return;
    if (isGroupChecked(g)) {
      onSelectionChange(selectedIds.filter((id) => !g.contractIds.includes(id)));
    } else {
      const merged = new Set([...selectedIds, ...g.contractIds]);
      onSelectionChange([...merged]);
    }
  };

  const selectedReadiness = useMemo(() => {
    const contractKey = viewMode === "billing" ? "client_contract_id" : "partner_contract_id";
    let ready = 0;
    let notReady = 0;
    for (const id of selectedIds) {
      const vr = rows.find((r) => r.row[contractKey] === id);
      if (!vr) continue;
      const checks = getReadinessChecks(vr, viewMode, monthLabel);
      if (isExcludedFromIssue(checks)) continue;
      if (isReadyForIssue(checks)) ready += 1;
      else notReady += 1;
    }
    return { ready, notReady };
  }, [selectedIds, rows, viewMode, monthLabel]);

  const amountColor = viewMode === "billing" ? "text-sky-400" : "text-amber-400";
  const accentBorder =
    viewMode === "billing" ? "border-t-sky-400" : "border-t-amber-400";
  const accentBg = viewMode === "billing" ? "bg-sky-500/5" : "bg-amber-500/5";
  const accentText = viewMode === "billing" ? "text-sky-400" : "text-amber-400";
  const parentBg = viewMode === "billing" ? "bg-sky-500/[0.04]" : "bg-amber-500/[0.04]";

  if (isLoading) {
    return (
      <div className="bg-card border border-border rounded-[0.625rem] p-6">
        <div className="space-y-3">
          {[...Array(4)].map((_, i) => (
            <div
              key={i}
              className="h-10 bg-muted/50 rounded animate-pulse"
              style={{ animationDelay: `${i * 80}ms` }}
            />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="bg-card border border-border rounded-[0.625rem] overflow-hidden shadow-sm">
      <div style={{ maxHeight: "calc(100vh - 420px)", overflowY: "auto" }}>
        <table className="w-full border-collapse text-[0.8125rem]">
          <thead>
            <tr>
              <th className="bg-background border-b-0" />
              <th
                colSpan={COLUMN_COUNT - 1}
                className={`text-center text-[0.625rem] font-semibold uppercase tracking-wider py-1 px-2 ${accentText} ${accentBg} border-t-2 ${accentBorder}`}
              >
                {viewMode === "billing" ? "⬆ 請求情報（売上）" : "⬇ 支払情報（原価）"}
              </th>
            </tr>
            <tr className="bg-background">
              <th className="text-center w-9 px-2 py-2.5 text-xs font-medium text-muted-foreground border-b border-border">
                <input
                  type="checkbox"
                  className="w-3.5 h-3.5 accent-primary cursor-pointer"
                  checked={allChecked}
                  onChange={toggleAll}
                  disabled={allSelectableIds.length === 0}
                />
              </th>
              <th className="px-3 py-2.5 text-left border-b border-border whitespace-nowrap">
                {viewMode === "billing" ? (
                  <SearchableColumnHeader
                    label="クライアント"
                    options={clients}
                    value={clientId}
                    onChange={onClientChange}
                    placeholder="クライアント名で検索…"
                  />
                ) : (
                  <SearchableColumnHeader
                    label="パートナー"
                    options={partners}
                    value={partnerId}
                    onChange={onPartnerChange}
                    placeholder="パートナー名で検索…"
                  />
                )}
              </th>
              <th className="px-3 py-2.5 text-left text-xs font-medium text-muted-foreground border-b border-border whitespace-nowrap">
                エンジニア
              </th>
              <th className="px-3 py-2.5 text-left border-b border-border whitespace-nowrap">
                <SearchableColumnHeader
                  label="対象年月"
                  options={availableMonths}
                  value={month}
                  onChange={onMonthChange}
                  placeholder="年月で検索…"
                />
              </th>
              <th className="px-3 py-2.5 text-right text-xs font-medium text-muted-foreground border-b border-border whitespace-nowrap">
                稼働時間
              </th>
              <th className="px-3 py-2.5 text-right text-xs font-medium text-muted-foreground border-b border-border whitespace-nowrap">
                {viewMode === "billing" ? "請求額合計" : "支払額合計"}
              </th>
              <th className="px-3 py-2.5 text-center text-xs font-medium text-muted-foreground border-b border-border whitespace-nowrap">
                {viewMode === "billing" ? "請求書" : "支払通知"}
              </th>
              <th className="px-3 py-2.5 text-left text-xs font-medium text-muted-foreground border-b border-border whitespace-nowrap">
                件数
              </th>
            </tr>
          </thead>
          <tbody>
            {groups.length === 0 ? (
              <tr>
                <td colSpan={COLUMN_COUNT} className="text-center py-12 text-muted-foreground">
                  データがありません
                </td>
              </tr>
            ) : (
              groups.map((g) => {
                const open = isExpanded(g.key);
                const groupChecked = isGroupChecked(g);
                const indeterminate = isGroupIndeterminate(g);
                const partnerIdForGroup = g.rows[0]?.row.partner_id ?? null;
                const groupCreatedNotices =
                  partnerIdForGroup != null
                    ? noticesByPartner.get(partnerIdForGroup) ?? []
                    : [];

                return (
                  <GroupRows
                    key={g.key}
                    group={g}
                    open={open}
                    viewMode={viewMode}
                    groupChecked={groupChecked}
                    indeterminate={indeterminate}
                    amountColor={amountColor}
                    parentBg={parentBg}
                    monthLabel={monthLabel}
                    createdNotices={groupCreatedNotices}
                    onDismissCreatedNotices={
                      partnerIdForGroup && onDismissCreatedNotices
                        ? () => onDismissCreatedNotices(partnerIdForGroup)
                        : undefined
                    }
                    onToggleExpand={() => toggleExpand(g.key)}
                    onToggleGroup={() => toggleGroup(g)}
                  />
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <div className="flex items-center justify-between px-4 py-3 border-t border-border bg-background/80">
        <div className="text-sm text-muted-foreground">
          ☑ <strong className="text-primary">{selectedIds.length}</strong> 件選択中
          {selectedIds.length > 0 && selectedReadiness.notReady > 0 && (
            <span className="ml-2 text-amber-400 text-xs">
              （うち {selectedReadiness.notReady} 件は条件未達）
            </span>
          )}
          {selectedIds.length > 0 && selectedReadiness.notReady === 0 && selectedReadiness.ready > 0 && (
            <span className="ml-2 text-emerald-400 text-xs">
              （すべて発行可能）
            </span>
          )}
          <span className="ml-2 text-xs block sm:inline mt-0.5 sm:mt-0">
            （{viewMode === "billing" ? "クライアント" : "パートナー"}単位チェック → 配下の契約を選択）
          </span>
        </div>
        <div className="flex gap-2">
          {viewMode === "billing" ? (
            <Button
              size="sm"
              className="bg-primary hover:bg-primary/90 text-primary-foreground gap-1"
              disabled={selectedIds.length === 0 || isIssuingInvoices}
              onClick={onIssueInvoices}
            >
              {isIssuingInvoices ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              請求書を一括発行
            </Button>
          ) : (
            <Button
              size="sm"
              className="bg-amber-500 hover:bg-amber-600 text-black gap-1"
              disabled={selectedIds.length === 0 || isIssuingNotices}
              onClick={onIssueNotices}
            >
              {isIssuingNotices ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              支払通知を一括発行
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

function GroupRows({
  group: g,
  open,
  viewMode,
  groupChecked,
  indeterminate,
  amountColor,
  parentBg,
  monthLabel,
  createdNotices = [],
  onDismissCreatedNotices,
  onToggleExpand,
  onToggleGroup,
}: {
  group: Group;
  open: boolean;
  viewMode: SettlementViewMode;
  groupChecked: boolean;
  indeterminate: boolean;
  amountColor: string;
  parentBg: string;
  monthLabel: string;
  createdNotices?: CreatedNotice[];
  onDismissCreatedNotices?: () => void;
  onToggleExpand: () => void;
  onToggleGroup: () => void;
}) {
  return (
    <>
      <tr className={`border-b border-border ${parentBg} hover:bg-accent/40 transition-colors`}>
        <td className="text-center px-2 py-2.5">
          <input
            type="checkbox"
            className="w-3.5 h-3.5 accent-primary cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            checked={groupChecked}
            ref={(el) => {
              if (el) el.indeterminate = indeterminate;
            }}
            onChange={onToggleGroup}
            disabled={!g.selectable}
            title={
              g.selectable
                ? undefined
                : g.allIssued
                  ? "支払通知は既に発行済みです"
                  : "自社エンジニアは支払通知の対象外です"
            }
          />
        </td>
        <td className="px-3 py-2.5">
          <button
            type="button"
            className="flex items-center gap-2 text-left w-full"
            onClick={onToggleExpand}
          >
            {open ? (
              <ChevronDown className="w-4 h-4 text-muted-foreground shrink-0" />
            ) : (
              <ChevronRight className="w-4 h-4 text-muted-foreground shrink-0" />
            )}
            <span className="font-semibold text-foreground">{g.label}</span>
            {!g.selectable && (
              <Badge className="bg-muted text-muted-foreground border-border text-[0.6rem] py-0 px-1.5">
                対象外
              </Badge>
            )}
          </button>
        </td>
        <td className="px-3 py-2.5 text-muted-foreground">—</td>
        <td className="px-3 py-2.5 text-muted-foreground whitespace-nowrap">
          {monthLabel}
        </td>
        <td className={`px-3 py-2.5 text-right tabular-nums font-medium ${g.allApproved ? "" : "text-muted-foreground"}`}>
          {formatHours(g.totalHours)}
        </td>
        <td className={`px-3 py-2.5 text-right tabular-nums font-semibold ${g.totalAmount > 0 ? amountColor : "text-muted-foreground"}`}>
          {g.totalAmount > 0 ? formatYen(g.totalAmount) : "—"}
        </td>
        <td className="px-3 py-2.5 text-center">
          <IssuedBadge allIssued={g.allIssued} anyIssued={g.anyIssued} />
        </td>
        <td className="px-3 py-2.5 text-muted-foreground text-xs">
          {g.rows.length} 名
          {g.issueTargetCount > 0 && (
            <span
              className={`block mt-0.5 tabular-nums ${
                g.readyCount === g.issueTargetCount ? "text-emerald-400" : "text-amber-400"
              }`}
            >
              発行可 {g.readyCount}/{g.issueTargetCount}
            </span>
          )}
        </td>
      </tr>

      {open &&
        g.rows.map((vr) => {
          const r = vr.row;
          const isApproved = r.timesheet_status === "APPROVED";
          const hasTimesheet = !!r.timesheet_id;
          const hours = parseHours(r.total_hours);
          const amount = viewMode === "billing" ? vr.billing_amount : vr.payment_amount;
          const rate = viewMode === "billing" ? r.billing_base_rate : r.payment_base_rate;
          const issued = viewMode === "billing" ? r.invoice_issued : r.notice_issued;
          const counterpart =
            viewMode === "billing"
              ? r.partner_name || "自社"
              : r.client_name;
          const rowKey =
            viewMode === "billing"
              ? `cc:${r.client_contract_id}`
              : `pc:${r.partner_contract_id ?? "none"}-cc:${r.client_contract_id}`;

          const documentLabel = viewMode === "billing" ? "請求書" : "支払通知";

          const handleUnapprovedClick = () => {
            if (hasTimesheet) {
              toast.error(
                `${r.engineer_name}さんの${monthLabel}分の稼働報告はまだ承認されていません。「稼働報告」画面で承認してから、${documentLabel}を発行してください。`
              );
            } else {
              toast.error(
                `${r.engineer_name}さんの${monthLabel}分の稼働報告がまだ提出されていません。提出・承認後に、${documentLabel}を発行してください。`
              );
            }
          };

          const guidance = getUnapprovedTimesheetGuidance(r as any, viewMode === "billing" ? "billing" : "payment", monthLabel);
          const readinessChecks = getReadinessChecks(vr, viewMode, monthLabel);

          return (
            <tr
              key={rowKey}
              className={`border-b border-border transition-colors hover:bg-accent/20 ${!isApproved ? "bg-rose-500/[0.03] cursor-pointer" : ""}`}
              onClick={!isApproved ? handleUnapprovedClick : undefined}
            >
              <td className="px-2 py-2" />
              <td className="px-3 py-2" />
              <td className="px-3 py-2 pl-6">
                <div className="flex items-center gap-2">
                  <span
                    className={`w-2 h-2 rounded-full flex-shrink-0 ${
                      isApproved ? "bg-emerald-400" : hasTimesheet ? "bg-amber-400" : "bg-rose-400"
                    }`}
                  />
                  <span className="font-medium text-foreground">{r.engineer_name}</span>
                  <ChildStatusBadge isApproved={isApproved} hasTimesheet={hasTimesheet} />
                  {!isApproved && (
                    <HelpTooltip
                      message={guidance.message}
                      linkLabel={guidance.linkLabel ?? "画面を開く"}
                      linkUrl={guidance.linkPath ?? "/timesheets"}
                    />
                  )}
                </div>
                <div className="text-[0.7rem] text-muted-foreground mt-0.5 ml-4">
                  {counterpart}
                  {r.project_name ? ` · ${r.project_name}` : ""}
                  {" · "}単金 {formatYen(rate)}
                </div>
                <IssueReadinessChips checks={readinessChecks} compact />
              </td>
              <td className="px-3 py-2 text-muted-foreground whitespace-nowrap">
                {monthLabel}
              </td>
              <td className="px-3 py-2 text-right tabular-nums">
                {!isApproved ? (
                  <span className="text-rose-400">—</span>
                ) : (
                  formatHours(hours)
                )}
              </td>
              <td
                className={`px-3 py-2 text-right tabular-nums font-semibold ${
                  isApproved ? amountColor : "text-muted-foreground"
                }`}
              >
                {isApproved ? formatYen(amount) : "—"}
              </td>
              <td className="px-3 py-2 text-center">
                <Badge
                  variant="outline"
                  className={
                    issued
                      ? "bg-emerald-500/15 text-emerald-400 border-emerald-500/30 text-[0.65rem]"
                      : "border-slate-500/30 text-slate-400 text-[0.65rem]"
                  }
                >
                  {issued ? "● 発行済" : "○ 未発行"}
                </Badge>
              </td>
              <td className="px-3 py-2" />
            </tr>
          );
        })}

      {open && viewMode === "payment" && createdNotices.length > 0 && onDismissCreatedNotices && (
        <tr className="border-b border-border">
          <td colSpan={COLUMN_COUNT} className="p-0 bg-amber-500/[0.02]">
            <CreatedNoticesInline notices={createdNotices} onDismiss={onDismissCreatedNotices} />
          </td>
        </tr>
      )}
    </>
  );
}
