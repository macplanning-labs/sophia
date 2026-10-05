"use client";

import type { SettlementSummary } from "@/lib/types";
import { SummaryCard, SummaryCardGrid } from "@/components/ui/summary-card";

interface Props {
  summary?: SettlementSummary;
  currentMonth: string;
  isLoading: boolean;
}

function formatYen(n: number): string {
  const abs = Math.abs(n);
  const formatted = abs.toLocaleString("ja-JP");
  return `¥${n < 0 ? "-" : ""}${formatted}`;
}

/** 税抜金額から税込金額を算出する（請求書作成時の計算式 subtotal + subtotal/10 と合わせる） */
function withTax(n: number): number {
  return n + Math.trunc(n / 10);
}

function YenWithTax({ amount }: { amount: number }) {
  return (
    <>
      {formatYen(amount)}
      <span className="text-sm font-normal text-muted-foreground ml-1.5">
        （税込 {formatYen(withTax(amount))}）
      </span>
    </>
  );
}

export function SummaryCards({ summary, currentMonth, isLoading }: Props) {
  if (isLoading || !summary) {
    return (
      <SummaryCardGrid cols={5}>
        {[...Array(5)].map((_, i) => (
          <div key={i} className="bg-card border border-border rounded-[0.625rem] px-4 py-3.5">
            <div className="h-3 w-20 bg-muted/50 rounded animate-pulse mb-2" />
            <div className="h-7 w-24 bg-muted/50 rounded animate-pulse" />
          </div>
        ))}
      </SummaryCardGrid>
    );
  }

  return (
    <SummaryCardGrid cols={5}>
      <SummaryCard
        label="対象月 契約数"
        value={summary.total_count}
        unit="件"
        sub={currentMonth}
        color="text-sky-400"
      />
      <SummaryCard
        label="売上合計（税抜）"
        value={<YenWithTax amount={summary.total_billing} />}
        sub={`請求書発行済: ${summary.invoice_issued_count}件 / 未: ${summary.invoice_pending_count}件`}
        color="text-sky-400"
      />
      <SummaryCard
        label="支払合計（税抜）"
        value={<YenWithTax amount={summary.total_payment} />}
        sub={`支払通知済: ${summary.notice_issued_count}件 / 未: ${summary.notice_pending_count}件`}
        color="text-amber-400"
      />
      <SummaryCard
        label="差額利益合計"
        value={formatYen(summary.total_profit)}
        sub={`平均利益率: ${summary.avg_profit_rate_display}`}
        color="text-emerald-400"
      />
      <SummaryCard
        label="未承認 稼働報告"
        value={summary.unapproved_count}
        unit="件"
        sub="⚠ 要確認"
        color="text-rose-400"
      />
    </SummaryCardGrid>
  );
}
