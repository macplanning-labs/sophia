"use client";

import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

// ── ステータス定義 ──

const STATUS_CONFIGS: Record<string, { label: string; className: string }> = {
  // 発注書
  DRAFT:       { label: "下書き",     className: "border-border text-muted-foreground" },
  SENT:        { label: "送付済",     className: "border-blue-500/30 text-blue-400" },
  ACCEPTED:    { label: "受諾済",     className: "border-emerald-500/30 text-emerald-400" },
  PUBLISHED:   { label: "確定済",     className: "border-purple-500/30 text-purple-400" },
  // 受注書
  REGISTERED:  { label: "登録済",     className: "border-amber-500/30 text-amber-400" },
  CONFIRMED:   { label: "確定",       className: "border-emerald-500/30 text-emerald-400" },
  INVOICED:    { label: "請求済",     className: "border-blue-500/30 text-blue-400" },
  INVOICE_SENT:     { label: "請求送付済", className: "border-blue-500/30 text-blue-400" },
  INVOICE_CONFIRMED:{ label: "受諾済",     className: "border-emerald-500/30 text-emerald-400" },
  // PAIDは発注書(orders/[id])のみがこのコンポーネント経由で表示する（受注書は`status_display`をバックエンドから
  // 受け取る別経路のため、ここには影響しない）。発注書のPAIDは「パートナーへの支払完了」を意味するため「支払済」とする
  PAID:        { label: "支払済",     className: "bg-emerald-500/20 text-emerald-400 border-emerald-500/30" },
  // 稼働報告
  PENDING:     { label: "未提出",     className: "border-border text-muted-foreground" },
  UPLOADED:    { label: "アップ済",   className: "border-amber-500/30 text-amber-400" },
  APPROVED:    { label: "承認済",     className: "border-emerald-500/30 text-emerald-400" },
  REJECTED:    { label: "差戻",       className: "border-red-500/30 text-red-400" },
  SENT_TO_CLIENT: { label: "送付済", className: "border-blue-500/30 text-blue-400" },
  // 支払通知
  REPORT_RECEIVED: { label: "報告受領", className: "border-amber-500/30 text-amber-400" },
  NOTICE_CREATED:  { label: "通知作成", className: "border-blue-500/30 text-blue-400" },
  NOTICE_CONFIRMED:{ label: "通知受諾", className: "border-emerald-500/30 text-emerald-400" },
  // 経費
  SUBMITTED:   { label: "申請中",     className: "border-amber-500/30 text-amber-400" },
  // 請求書承認・送信ワークフロー（SENTは発注書の「送付済」を流用）
  PENDING_APPROVAL: { label: "承認待ち", className: "border-amber-500/30 text-amber-400" },
  // 給与
  CALCULATED:  { label: "計算済",     className: "border-amber-500/30 text-amber-400" },
  // 共通
  CANCELLED:   { label: "取消",       className: "border-red-500/30 text-red-400" },
  ACTIVE:      { label: "有効",       className: "border-emerald-500/30 text-emerald-400" },
  INACTIVE:    { label: "無効",       className: "border-border text-muted-foreground" },
};

interface StatusBadgeProps {
  status: string;
  className?: string;
  /** カスタムラベル。指定しない場合はSTATUS_CONFIGSから自動取得 */
  label?: string;
}

export function StatusBadge({ status, className, label }: StatusBadgeProps) {
  const config = STATUS_CONFIGS[status] ?? {
    label: status,
    className: "border-border text-muted-foreground",
  };

  return (
    <Badge
      variant="outline"
      className={cn("text-[10px] font-medium", config.className, className)}
    >
      {label ?? config.label}
    </Badge>
  );
}

/** ステータスのラベルを取得 */
export function getStatusLabel(status: string): string {
  return STATUS_CONFIGS[status]?.label ?? status;
}
