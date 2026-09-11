"use client";

import { Suspense, useState, useMemo, useCallback } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { useSettlement, useFilters, useIssueInvoices, useIssueNotices } from "@/hooks/use-settlement";
import { SummaryCards } from "@/components/settlement/summary-cards";
import {
  SettlementTable,
  type SettlementViewMode,
} from "@/components/settlement/settlement-table";
import { RefreshCw, FileUp, FileDown } from "lucide-react";
import { PageHeader } from "@/components/ui/page-header";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { CreatedInvoicesModal } from "@/components/settlement/created-invoices-modal";
import { SettlementReadinessPanel } from "@/components/settlement/settlement-readiness-panel";
import { showActionableError } from "@/lib/actionable-error";
import type { CreatedInvoice, CreatedNotice } from "@/lib/types";

export default function SettlementPage() {
  return (
    <Suspense fallback={<div className="p-8 text-sm text-muted-foreground">読み込み中...</div>}>
      <SettlementPageContent />
    </Suspense>
  );
}

function SettlementPageContent() {
  const searchParams = useSearchParams();

  const [month, setMonth] = useState<string>(() => {
    const fromUrl = searchParams.get("month");
    if (fromUrl) return fromUrl;
    const now = new Date();
    return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
  });
  const [clientId, setClientId] = useState<string>("");
  const [partnerId, setPartnerId] = useState<string>("");
  const [projectId] = useState<string>(() => searchParams.get("project_id") ?? "");
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [viewMode, setViewMode] = useState<SettlementViewMode>("billing");
  const [createdInvoices, setCreatedInvoices] = useState<CreatedInvoice[]>([]);
  const [createdNotices, setCreatedNotices] = useState<CreatedNotice[]>([]);

  const params = useMemo(
    () => ({
      month: month || undefined,
      client_id: clientId || undefined,
      partner_id: partnerId || undefined,
      project_id: projectId || undefined,
    }),
    [month, clientId, partnerId, projectId]
  );

  const { data, isLoading, isFetching, refetch } = useSettlement(params);
  const { data: filters } = useFilters();

  const invoiceMutation = useIssueInvoices(month);
  const noticeMutation = useIssueNotices(month);

  const handleViewModeChange = useCallback((mode: SettlementViewMode) => {
    setViewMode(mode);
    setSelectedIds([]);
  }, []);

  const handleMonthChange = useCallback((value: string) => {
    if (value) setMonth(value);
  }, []);

  const clientOptions = useMemo(
    () => (filters?.clients ?? []).map((c) => ({ value: String(c.id), label: c.name })),
    [filters?.clients]
  );

  const partnerOptions = useMemo(
    () => (filters?.partners ?? []).map((p) => ({ value: p.partner_id, label: p.name })),
    [filters?.partners]
  );

  const monthLabel = useMemo(() => {
    const m = month.match(/^(\d{4})-(\d{2})/);
    return m ? `${m[1]}年${m[2]}月` : month;
  }, [month]);

  const handleIssueInvoices = () => {
    if (selectedIds.length === 0) return;
    invoiceMutation.mutate(selectedIds, {
      onSuccess: (result) => {
        if (result.success) {
          setSelectedIds([]);
          if (result.created_invoices && result.created_invoices.length > 0) {
            setCreatedInvoices(result.created_invoices);
          } else {
            toast.success(result.message ?? "請求書を発行しました");
          }
        } else {
          showActionableError(toast, result, "請求書を発行できませんでした");
        }
      },
      onError: (e: Error) => toast.error(`請求書の発行に失敗しました: ${e.message}`),
    });
  };

  const handleIssueNotices = () => {
    if (selectedIds.length === 0) return;
    noticeMutation.mutate(selectedIds, {
      onSuccess: (result) => {
        if (result.success) {
          setSelectedIds([]);
          if (result.created_notices && result.created_notices.length > 0) {
            setCreatedNotices(result.created_notices);
          } else {
            toast.success(result.message ?? "支払通知書を発行しました");
          }
        } else {
          showActionableError(toast, result, "支払通知を発行できませんでした");
        }
      },
      onError: (e: Error) => toast.error(`支払通知書の発行に失敗しました: ${e.message}`),
    });
  };

  const handleDismissCreatedNotices = useCallback((partnerId: string) => {
    setCreatedNotices((prev) => prev.filter((n) => n.partner_id !== partnerId));
  }, []);

  return (
    <div className="p-6">
      <PageHeader
        actions={
          <>
            {viewMode === "billing" ? (
              <>
                <Link href="/received-orders">
                  <Button variant="outline" size="sm" className="border-border text-muted-foreground">
                    受注一覧
                  </Button>
                </Link>
                <Link href="/invoices">
                  <Button variant="outline" size="sm" className="border-border text-muted-foreground">
                    請求書一覧
                  </Button>
                </Link>
              </>
            ) : (
              <>
                <Link href="/orders">
                  <Button variant="outline" size="sm" className="border-border text-muted-foreground">
                    発注一覧
                  </Button>
                </Link>
                <Link href="/notices">
                  <Button variant="outline" size="sm" className="border-border text-muted-foreground">
                    支払通知一覧
                  </Button>
                </Link>
              </>
            )}
            <Button
              variant="outline"
              size="sm"
              className="border-border text-muted-foreground gap-1"
              onClick={() => refetch()}
              disabled={isFetching}
            >
              <RefreshCw className={`w-3.5 h-3.5 ${isFetching ? "animate-spin" : ""}`} />
              更新
            </Button>
          </>
        }
      />

      <SummaryCards
        summary={data?.summary}
        currentMonth={data?.current_month ?? ""}
        isLoading={isLoading}
      />

      {/* 請求 / 支払 ビュー切替 */}
      <div className="flex gap-1 mb-4 border-b border-border">
        <button
          type="button"
          onClick={() => handleViewModeChange("billing")}
          className={cn(
            "flex items-center gap-2 px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors",
            viewMode === "billing"
              ? "border-sky-400 text-sky-400"
              : "border-transparent text-muted-foreground hover:text-foreground"
          )}
        >
          <FileUp className="w-4 h-4" />
          請求（対クライアント）
        </button>
        <button
          type="button"
          onClick={() => handleViewModeChange("payment")}
          className={cn(
            "flex items-center gap-2 px-4 py-2.5 text-sm font-medium border-b-2 -mb-px transition-colors",
            viewMode === "payment"
              ? "border-amber-400 text-amber-400"
              : "border-transparent text-muted-foreground hover:text-foreground"
          )}
        >
          <FileDown className="w-4 h-4" />
          支払（対パートナー）
        </button>
      </div>

      {/* 一括発行の準備状況 */}
      {!isLoading && (data?.rows?.length ?? 0) > 0 && (
        <SettlementReadinessPanel
          rows={data?.rows ?? []}
          viewMode={viewMode}
          monthLabel={monthLabel}
        />
      )}

      <SettlementTable
        key={viewMode}
        rows={data?.rows ?? []}
        isLoading={isLoading}
        viewMode={viewMode}
        selectedIds={selectedIds}
        onSelectionChange={setSelectedIds}
        onIssueInvoices={handleIssueInvoices}
        onIssueNotices={handleIssueNotices}
        isIssuingInvoices={invoiceMutation.isPending}
        isIssuingNotices={noticeMutation.isPending}
        month={month}
        onMonthChange={handleMonthChange}
        availableMonths={filters?.available_months ?? []}
        clientId={clientId}
        onClientChange={setClientId}
        clients={clientOptions}
        partnerId={partnerId}
        onPartnerChange={setPartnerId}
        partners={partnerOptions}
        createdNotices={createdNotices}
        onDismissCreatedNotices={handleDismissCreatedNotices}
      />

      <CreatedInvoicesModal
        open={createdInvoices.length > 0}
        onClose={() => setCreatedInvoices([])}
        invoices={createdInvoices}
      />
    </div>
  );
}
