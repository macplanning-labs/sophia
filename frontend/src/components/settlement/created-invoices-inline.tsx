"use client";

import { useEffect, useMemo, useState } from "react";
import { X } from "lucide-react";
import type { CreatedInvoice } from "@/lib/types";
import InvoiceDetailPage from "@/app/invoices/[id]/client";

interface Props {
  invoices: CreatedInvoice[];
  onDismiss: () => void;
}

function formatYen(n: number): string {
  return `¥${n.toLocaleString("ja-JP")}`;
}

/** 請求書一括発行直後、クライアント行の下に表示するインラインパネル。
 *  PDFプレビュー・承認・送信までこの場で続けられるようにする。 */
export function CreatedInvoicesInline({ invoices: all, onDismiss }: Props) {
  const [removed, setRemoved] = useState<Set<string>>(new Set());
  const invoices = useMemo(() => all.filter((x) => !removed.has(String(x.id))), [all, removed]);
  const [activeId, setActiveId] = useState<number | null>(invoices[0]?.id ?? null);

  useEffect(() => {
    setActiveId(invoices[0]?.id ?? null);
  }, [invoices]);

  const handleDeleted = (id: string) => {
    const rest = invoices.filter((x) => String(x.id) !== id);
    setRemoved((prev) => new Set(prev).add(id));
    if (rest.length === 0) onDismiss();
  };

  if (invoices.length === 0) return null;

  return (
    <div className="mx-3 my-3 rounded-lg border border-sky-500/40 bg-sky-500/[0.06] overflow-hidden">
      <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-sky-500/25 bg-sky-500/[0.08]">
        <div className="min-w-0">
          <p className="text-sm font-semibold text-foreground">
            請求書を{invoices.length}件作成しました
          </p>
          <p className="text-xs text-muted-foreground mt-0.5">
            まだクライアントには送信されていません。内容を確認のうえ、承認・送信してください。
          </p>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          className="text-muted-foreground hover:text-foreground transition-colors p-1 shrink-0"
          aria-label="パネルを閉じる"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {invoices.length > 1 && (
        <div className="flex items-center gap-1 px-3 pt-2 overflow-x-auto border-b border-sky-500/20">
          {invoices.map((n) => (
            <button
              key={n.id}
              type="button"
              onClick={() => setActiveId(n.id)}
              className={`px-3 py-1.5 text-xs font-medium rounded-t-md border-b-2 whitespace-nowrap transition-colors ${
                activeId === n.id
                  ? "border-sky-400 text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {n.project_name ? `${n.project_name} · ` : ""}
              {formatYen(n.total)}
              {" · 要承認"}
            </button>
          ))}
        </div>
      )}

      <div className="px-3 py-3 max-h-[min(60vh,520px)] overflow-y-auto">
        {activeId != null && (
          <InvoiceDetailPage key={activeId} invoiceId={String(activeId)} embedded onDeleted={() => handleDeleted(String(activeId))} />
        )}
      </div>
    </div>
  );
}
