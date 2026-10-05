"use client";

import { Suspense, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useCurrentMonth } from "@/lib/MonthContext";
import { fetchBillingPreview, confirmBilling } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ConfirmSheet, type ConfirmFact } from "@/components/ui/confirm-sheet";
import { FormModal, FormField, FormTextarea, FormSelect } from "@/components/ui/form-modal";
import { ChevronDown, ChevronRight } from "lucide-react";
import Link from "next/link";
import { toast } from "sonner";
import type { BillingInvoicePreview, BillingConfirmRequest, ForceAction } from "@/lib/types";

export default function BillingPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <BillingPageContent />
    </Suspense>
  );
}

// Format price with thousand separators
function formatPrice(amount: number): string {
  return `¥${amount.toLocaleString("ja-JP")}`;
}

// Get status display for an invoice
function getInvoiceStatus(invoice: BillingInvoicePreview): string {
  if (invoice.edi_excluded) {
    return "EDI連携のため対象外";
  }
  if (invoice.confirmable) {
    return "確定できます";
  }
  if (invoice.items.length === 0) {
    const suffix = invoice.already_issued_count > 0
      ? ` (発行済 ${invoice.already_issued_count}名)`
      : "";
    return `確定する明細がありません${suffix}`;
  }
  // Has missing items
  return `あと${invoice.missing.length}名`;
}

interface ExpandState {
  [key: string]: boolean;
}

function BillingPageContent() {
  const { month } = useCurrentMonth();
  const queryClient = useQueryClient();
  const [expandState, setExpandState] = useState<ExpandState>({});
  const [confirmTarget, setConfirmTarget] = useState<BillingInvoicePreview | null>(null);
  const [forceConfirmTarget, setForceConfirmTarget] = useState<BillingInvoicePreview | null>(null);
  const [forceReason, setForceReason] = useState("");
  const [forceActions, setForceActions] = useState<Record<number, "NEXT_MONTH" | "SECOND_INVOICE">>({});

  const { data: preview, isLoading } = useQuery({
    queryKey: ["billing-preview", month],
    queryFn: () => fetchBillingPreview(month),
  });

  const invoices = preview?.invoices ?? [];

  // Confirm mutation
  const confirmMutation = useMutation({
    mutationFn: async (req: BillingConfirmRequest) => {
      return confirmBilling(req);
    },
    onSuccess: (result) => {
      if (result.success) {
        queryClient.invalidateQueries({ queryKey: ["billing-preview", month] });
        setConfirmTarget(null);
        setForceConfirmTarget(null);
        setForceReason("");
        setForceActions({});

        // Show invoice numbers in toast
        if (result.created.length > 0) {
          const invoiceNos = result.created.map((inv) => inv.invoice_no).join("、");
          toast.success(`請求書を確定しました: ${invoiceNos}`);
        } else {
          toast.success("請求書を確定しました");
        }
      } else {
        toast.error(result.error || "確定に失敗しました");
      }
    },
    onError: (err) => {
      toast.error(`確定に失敗しました: ${err instanceof Error ? err.message : "不明なエラー"}`);
    },
  });

  const handleToggleExpand = (key: string) => {
    setExpandState((prev) => ({
      ...prev,
      [key]: !prev[key],
    }));
  };

  const handleConfirmClick = (invoice: BillingInvoicePreview) => {
    setConfirmTarget(invoice);
  };

  const handleForceConfirmClick = (invoice: BillingInvoicePreview) => {
    setForceConfirmTarget(invoice);
    setForceReason("");
    setForceActions({});
  };

  const handleConfirmSubmit = async () => {
    if (!confirmTarget) return;

    const req: BillingConfirmRequest = {
      month,
      keys: [confirmTarget.key],
      force: false,
    };

    confirmMutation.mutate(req);
  };

  const handleForceConfirmSubmit = async () => {
    if (!forceConfirmTarget) return;

    // Validate reason
    if (!forceReason.trim()) {
      toast.error("理由を入力してください");
      return;
    }

    if (forceReason.trim().length > 500) {
      toast.error("理由は500文字以内で入力してください");
      return;
    }

    // Check all missing contracts have an action
    const uncovered = forceConfirmTarget.missing.filter(
      (m) => !(m.client_contract_id in forceActions)
    );
    if (uncovered.length > 0) {
      toast.error("すべての揃っていない要員の扱いを指定してください");
      return;
    }

    const actions: ForceAction[] = forceConfirmTarget.missing.map((m) => ({
      client_contract_id: m.client_contract_id,
      action: forceActions[m.client_contract_id],
    }));

    const req: BillingConfirmRequest = {
      month,
      keys: [forceConfirmTarget.key],
      force: true,
      force_reason: forceReason.trim(),
      force_actions: actions,
    };

    confirmMutation.mutate(req);
  };

  const confirmFacts: ConfirmFact[] | undefined = useMemo(() => {
    if (!confirmTarget) return undefined;
    return [
      { label: "取引先", value: confirmTarget.client_name },
      { label: "件名", value: confirmTarget.subject },
      { label: "明細数", value: confirmTarget.items.length },
      { label: "合計(税込)", value: formatPrice(confirmTarget.total) },
    ];
  }, [confirmTarget]);

  return (
    <div className="space-y-6 p-6">
      {/* Info box about notices */}
      <div className="rounded-md bg-blue-500/10 border border-blue-500/30 p-3">
        <p className="text-xs text-blue-300">
          支払通知は{" "}
          <Link href="/notices" className="text-blue-400 hover:text-blue-300 underline">
            従来の画面
          </Link>
          で確認します
        </p>
      </div>

      {/* Loading state */}
      {isLoading && (
        <div className="text-sm text-muted-foreground">読み込み中...</div>
      )}

      {/* Empty state */}
      {!isLoading && invoices.length === 0 && (
        <div className="text-sm text-muted-foreground">確定対象の請求書がありません</div>
      )}

      {/* Invoices table */}
      {!isLoading && invoices.length > 0 && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-6"></TableHead>
              <TableHead>取引先</TableHead>
              <TableHead>案件</TableHead>
              <TableHead className="text-right">明細</TableHead>
              <TableHead className="text-right">合計(税込)</TableHead>
              <TableHead>状況</TableHead>
              <TableHead className="w-24">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {invoices.map((invoice) => {
              const isExpanded = expandState[invoice.key] ?? false;
              const canConfirm = invoice.confirmable && !invoice.edi_excluded;
              const canForceConfirm =
                !invoice.confirmable &&
                invoice.items.length > 0 &&
                !invoice.edi_excluded &&
                invoice.missing.length > 0;

              return (
                <div key={invoice.key}>
                  {/* Main row */}
                  <TableRow
                    className="cursor-pointer hover:bg-muted/50"
                    onClick={() => handleToggleExpand(invoice.key)}
                  >
                    <TableCell>
                      {invoice.items.length > 0 || invoice.missing.length > 0 ? (
                        isExpanded ? (
                          <ChevronDown className="w-4 h-4" />
                        ) : (
                          <ChevronRight className="w-4 h-4" />
                        )
                      ) : null}
                    </TableCell>
                    <TableCell className="text-sm">{invoice.client_name}</TableCell>
                    <TableCell className="text-sm">
                      {invoice.project_id ? invoice.subject.split(" ")[0] : "全案件"}
                    </TableCell>
                    <TableCell className="text-right text-sm">{invoice.items.length}</TableCell>
                    <TableCell className="text-right text-sm font-medium">
                      {formatPrice(invoice.total)}
                    </TableCell>
                    <TableCell>
                      <span className="text-xs text-muted-foreground">
                        {getInvoiceStatus(invoice)}
                      </span>
                    </TableCell>
                    <TableCell>
                      {canConfirm && (
                        <Button
                          size="sm"
                          variant="default"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleConfirmClick(invoice);
                          }}
                          className="text-xs"
                        >
                          確定
                        </Button>
                      )}
                      {canForceConfirm && (
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleForceConfirmClick(invoice);
                          }}
                          className="text-xs"
                        >
                          待たずに確定
                        </Button>
                      )}
                    </TableCell>
                  </TableRow>

                  {/* Expanded details */}
                  {isExpanded && (invoice.items.length > 0 || invoice.missing.length > 0) && (
                    <TableRow className="bg-muted/20">
                      <TableCell colSpan={7} className="p-4">
                        <div className="space-y-4">
                          {/* Items */}
                          {invoice.items.length > 0 && (
                            <div>
                              <h4 className="text-xs font-semibold text-foreground mb-2">
                                明細 ({invoice.items.length}件)
                              </h4>
                              <div className="space-y-1">
                                {invoice.items.map((item, idx) => (
                                  <div
                                    key={idx}
                                    className="text-xs text-muted-foreground flex justify-between"
                                  >
                                    <span>{item.engineer_name}</span>
                                    <span>{formatPrice(item.amount)}</span>
                                  </div>
                                ))}
                              </div>
                            </div>
                          )}

                          {/* Missing items */}
                          {invoice.missing.length > 0 && (
                            <div>
                              <h4 className="text-xs font-semibold text-rose-400 mb-2">
                                揃っていない要員 ({invoice.missing.length}名)
                              </h4>
                              <div className="space-y-1">
                                {invoice.missing.map((item, idx) => (
                                  <div
                                    key={idx}
                                    className="text-xs text-muted-foreground flex justify-between"
                                  >
                                    <span>{item.engineer_name}</span>
                                    <span className="text-rose-400">
                                      {item.timesheet_status || "不明"}
                                    </span>
                                  </div>
                                ))}
                              </div>
                            </div>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  )}
                </div>
              );
            })}
          </TableBody>
        </Table>
      )}

      {/* Confirm dialog */}
      <ConfirmSheet
        open={!!confirmTarget}
        title="請求書を確定"
        facts={confirmFacts}
        confirmLabel="確定"
        loading={confirmMutation.isPending}
        onConfirm={handleConfirmSubmit}
        onCancel={() => setConfirmTarget(null)}
      />

      {/* Force confirm modal */}
      <FormModal
        open={!!forceConfirmTarget}
        title="例外で確定"
        size="md"
        submitLabel="例外で確定"
        loading={confirmMutation.isPending}
        onSubmit={handleForceConfirmSubmit}
        onClose={() => {
          setForceConfirmTarget(null);
          setForceReason("");
          setForceActions({});
        }}
      >
        <div className="space-y-4">
          {/* Reason */}
          <FormField label="理由" required error={forceReason.length > 500 ? "500文字以内で入力してください" : undefined}>
            <FormTextarea
              value={forceReason}
              onChange={(e) => setForceReason(e.target.value)}
              placeholder="例外で確定する理由を入力してください"
              maxLength={500}
            />
          </FormField>

          {/* Missing items actions */}
          {forceConfirmTarget && forceConfirmTarget.missing.length > 0 && (
            <div className="space-y-3">
              <p className="text-xs font-semibold text-foreground">
                揃っていない要員の扱い
              </p>
              {forceConfirmTarget.missing.map((item) => (
                <FormField
                  key={item.client_contract_id}
                  label={item.engineer_name}
                  required
                >
                  <FormSelect
                    value={forceActions[item.client_contract_id] || ""}
                    onChange={(e) => {
                      setForceActions((prev) => ({
                        ...prev,
                        [item.client_contract_id]: e.target.value as "NEXT_MONTH" | "SECOND_INVOICE",
                      }));
                    }}
                    options={[
                      { value: "NEXT_MONTH", label: "翌月回し" },
                      { value: "SECOND_INVOICE", label: "当月2通目" },
                    ]}
                    placeholder="選択してください"
                  />
                </FormField>
              ))}
            </div>
          )}
        </div>
      </FormModal>
    </div>
  );
}
