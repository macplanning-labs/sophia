"use client";

import { useEffect, useState } from "react";
import { X } from "lucide-react";
import type { CreatedInvoice } from "@/lib/types";
import InvoiceDetailPage from "@/app/invoices/[id]/client";

interface Props {
  open: boolean;
  onClose: () => void;
  invoices: CreatedInvoice[];
}

function formatYen(n: number): string {
  return `¥${n.toLocaleString("ja-JP")}`;
}

/** 請求書一括発行の直後に表示する確認モーダル。作成された請求書をタブで切り替えながら
 *  PDFプレビュー・承認・送信までこの場で一元管理できるようにする
 *  （発行しただけでは誰にも送信されていないことが伝わりにくいという指摘への対応）。 */
export function CreatedInvoicesModal({ open, onClose, invoices }: Props) {
  const [activeId, setActiveId] = useState<number | null>(invoices[0]?.id ?? null);

  useEffect(() => {
    if (open) setActiveId(invoices[0]?.id ?? null);
  }, [open, invoices]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open || invoices.length === 0) return null;

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
            <h3 className="text-base font-semibold text-foreground">
              請求書を{invoices.length}件作成しました
            </h3>
            <p className="text-xs text-muted-foreground mt-1">
              まだクライアントには送信されていません。内容を確認のうえ、承認・送信してください。
            </p>
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

        {invoices.length > 1 && (
          <div className="flex items-center gap-1 px-4 pt-3 overflow-x-auto shrink-0 border-b border-border">
            {invoices.map((inv) => (
              <button
                key={inv.id}
                type="button"
                onClick={() => setActiveId(inv.id)}
                className={`px-3 py-1.5 text-xs font-medium rounded-t-md border-b-2 whitespace-nowrap transition-colors ${
                  activeId === inv.id
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                }`}
              >
                {inv.client_name} {inv.project_name ? `· ${inv.project_name}` : ""} · {formatYen(inv.total)}
              </button>
            ))}
          </div>
        )}

        <div className="px-4 py-4 overflow-y-auto flex-1 min-h-0">
          {activeId != null && (
            <InvoiceDetailPage key={activeId} invoiceId={String(activeId)} embedded />
          )}
        </div>
      </div>
    </div>
  );
}
