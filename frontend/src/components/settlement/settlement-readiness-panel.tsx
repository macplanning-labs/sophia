"use client";

import Link from "next/link";
import type { SettlementViewRow } from "@/lib/types";
import {
  collectReadinessIssues,
  type ReadinessCheck,
} from "@/lib/guidance/settlement-readiness";
import type { SettlementViewMode } from "@/components/settlement/settlement-table";
import { CheckCircle2, AlertTriangle, Circle, MinusCircle } from "lucide-react";

interface Props {
  rows: SettlementViewRow[];
  viewMode: SettlementViewMode;
  monthLabel: string;
}

function StatusIcon({ status }: { status: ReadinessCheck["status"] }) {
  switch (status) {
    case "ok":
      return <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />;
    case "done":
      return <CheckCircle2 className="w-3.5 h-3.5 text-sky-400 shrink-0" />;
    case "warning":
      return <AlertTriangle className="w-3.5 h-3.5 text-amber-400 shrink-0" />;
    case "na":
      return <MinusCircle className="w-3.5 h-3.5 text-muted-foreground shrink-0" />;
    default:
      return <Circle className="w-3.5 h-3.5 text-rose-400 shrink-0" />;
  }
}

export function SettlementReadinessPanel({ rows, viewMode, monthLabel }: Props) {
  const mode = viewMode === "billing" ? "billing" : "payment";
  const { readyCount, issueCount, totalCount, issues } = collectReadinessIssues(
    rows,
    mode,
    monthLabel
  );

  if (totalCount === 0) return null;

  const docLabel = viewMode === "billing" ? "請求書" : "支払通知";
  const allReady = issueCount === 0;

  return (
    <div
      className={`mb-4 rounded-lg border px-4 py-3 ${
        allReady
          ? "border-emerald-500/30 bg-emerald-500/[0.06]"
          : "border-amber-500/35 bg-amber-500/[0.06]"
      }`}
    >
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="text-sm font-semibold text-foreground">
            {docLabel}一括発行の準備状況
          </p>
          <p className="text-xs text-muted-foreground mt-0.5">
            {viewMode === "billing"
              ? "対象月の稼働報告が承認済みである必要があります。"
              : "発注契約・稼働報告承認・発注書（受諾済または報告書受領）が必要です。"}
          </p>
        </div>
        <div className="text-right shrink-0">
          <p className={`text-lg font-bold tabular-nums ${allReady ? "text-emerald-400" : "text-amber-400"}`}>
            {readyCount}/{totalCount}
          </p>
          <p className="text-[0.65rem] text-muted-foreground">件 発行可能</p>
        </div>
      </div>

      {allReady ? (
        <p className="text-xs text-emerald-400/90 mt-2">
          すべての対象契約で一括発行の条件が整っています。パートナー／クライアント単位で選択して発行してください。
        </p>
      ) : (
        <ul className="mt-3 space-y-2">
          {issues.map((issue, i) => (
            <li
              key={`${issue.engineerName}-${issue.check.id}-${i}`}
              className="flex items-start gap-2 text-xs rounded-md bg-background/60 border border-border/60 px-3 py-2"
            >
              <StatusIcon status={issue.check.status} />
              <div className="min-w-0 flex-1">
                <p className="font-medium text-foreground">
                  {issue.engineerName}
                  <span className="font-normal text-muted-foreground ml-1.5">
                    {issue.context}
                  </span>
                </p>
                <p className="text-muted-foreground mt-0.5">
                  {issue.check.label}: {issue.check.detail}
                </p>
                {issue.check.linkPath && issue.check.linkLabel && (
                  <Link
                    href={issue.check.linkPath}
                    className="inline-block mt-1 text-sky-400 hover:text-sky-300 underline font-medium"
                  >
                    {issue.check.linkLabel} →
                  </Link>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
