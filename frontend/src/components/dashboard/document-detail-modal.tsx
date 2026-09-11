"use client";

import { useEffect, useMemo } from "react";
import { X } from "lucide-react";
import type { ProgressRow, SettlementViewRow } from "@/lib/types";
import OrderDetailPage from "@/app/orders/[id]/client";
import ReceivedOrderDetailPage from "@/app/received-orders/[id]/client";
import Link from "next/link";

/** 帳票詳細モーダル（発注書 / 受注書） */
export interface DocumentModalTarget {
  kind: "purchase" | "received";
  engineerName: string;
  /** 発注書ID or 受注書ID */
  orderId: string;
  /** 作業者一覧から渡す精算行（ヘッダ金額表示用・任意） */
  engineer?: SettlementViewRow;
  /** 進捗の対象月 "2026/08" */
  progressMonth?: string;
}

interface Props {
  open: boolean;
  onClose: () => void;
  target: DocumentModalTarget;
  projectName: string;
  clientName: string;
  partnerProgress: ProgressRow[];
  clientProgress?: ProgressRow[];
}

function formatYen(n: number): string {
  return `¥${n < 0 ? "-" : ""}${Math.abs(n).toLocaleString("ja-JP")}`;
}

export function DocumentDetailModal({
  open,
  onClose,
  target,
  projectName,
  clientName,
  partnerProgress,
  clientProgress = [],
}: Props) {
  const isPurchase = target.kind === "purchase";
  const poRow = useMemo(
    () => (isPurchase ? partnerProgress.find((r) => r.order_id === target.orderId) : undefined),
    [isPurchase, partnerProgress, target.orderId]
  );
  const roRow = useMemo(
    () => (!isPurchase ? clientProgress.find((r) => r.order_id === target.orderId) : undefined),
    [isPurchase, clientProgress, target.orderId]
  );
  const settlement = target.engineer;

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const subtitleParts = [
    projectName,
    clientName,
    settlement?.row.partner_name || poRow?.entity_name || roRow?.entity_name || undefined,
    target.progressMonth || undefined,
    settlement
      ? `請求 ${formatYen(settlement.billing_amount)}${
          settlement.row.partner_contract_id != null
            ? ` / 支払 ${formatYen(settlement.payment_amount)}`
            : ""
        }`
      : undefined,
  ].filter(Boolean);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[4vh] bg-black/60 backdrop-blur-sm overflow-y-auto"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="bg-card border border-border rounded-xl shadow-2xl w-full max-w-5xl mx-4 mb-10 animate-in fade-in zoom-in-95 duration-200 flex flex-col max-h-[92vh]">
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-3 border-b border-border shrink-0">
          <div className="min-w-0">
            <h3 className="text-base font-semibold text-foreground truncate">
              {isPurchase ? "発注詳細" : "受注詳細"} — {target.engineerName}
            </h3>
            <p className="text-xs text-muted-foreground mt-1 truncate">
              {subtitleParts.join(" · ")}
            </p>
            {isPurchase && (
              <p className="text-[10px] text-muted-foreground/80 mt-1">
                個人単位の発注書操作のみ。支払通知・請求書はパートナー／クライアント単位のため月次確定などから発行します。
              </p>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground transition-colors p-1"
            aria-label="閉じる"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="px-4 py-4 overflow-y-auto flex-1 min-h-0">
          {isPurchase ? (
            target.orderId ? (
              <OrderDetailPage
                key={target.orderId}
                orderId={target.orderId}
                embedded
                onDeleted={onClose}
              />
            ) : (
              <div className="rounded-lg border border-dashed border-border bg-muted/20 px-6 py-10 text-center">
                <p className="text-sm font-medium text-foreground mb-1">発注書がありません</p>
                <p className="text-xs text-muted-foreground mb-4">
                  この作業者の発注書はまだ作成されていません。
                </p>
                <Link
                  href="/partner-contracts"
                  className="inline-flex text-xs px-3 py-1.5 rounded-md border border-sky-500/30 text-sky-400 hover:bg-sky-500/10"
                >
                  発注契約へ
                </Link>
              </div>
            )
          ) : target.orderId ? (
            <ReceivedOrderDetailPage
              key={target.orderId}
              orderId={target.orderId}
              embedded
              onDeleted={onClose}
            />
          ) : (
            <div className="rounded-lg border border-dashed border-border bg-muted/20 px-6 py-10 text-center">
              <p className="text-sm font-medium text-foreground mb-1">受注書がありません</p>
              <p className="text-xs text-muted-foreground mb-4">
                この作業者の受注書はまだ作成されていません。
              </p>
              <Link
                href="/client-contracts"
                className="inline-flex text-xs px-3 py-1.5 rounded-md border border-sky-500/30 text-sky-400 hover:bg-sky-500/10"
              >
                受注契約へ
              </Link>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
