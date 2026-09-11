"use client";

import { Suspense, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchReceivedOrders, deleteReceivedOrder } from "@/lib/api";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import Link from "next/link";
import { toast } from "sonner";
import type { ReceivedOrderRow } from "@/lib/types";

export default function ReceivedOrdersPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <ReceivedOrdersPageContent />
    </Suspense>
  );
}

function ReceivedOrdersPageContent() {
  const [statusFilter, setStatusFilter] = useState("");
  const [clientFilter, setClientFilter] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [isDeleting, setIsDeleting] = useState(false);
  const qc = useQueryClient();

  const { data: receivedOrders, isLoading } = useQuery({
    queryKey: ["receivedOrders"],
    queryFn: fetchReceivedOrders,
  });

  const rows = receivedOrders ?? [];

  const statusOptions = useMemo(() => {
    const statuses = new Set(rows.map((r) => r.status).filter(Boolean));
    return [...statuses]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((s) => ({ value: s, label: s }));
  }, [rows]);

  const clientOptions = useMemo(() => {
    const clients = new Set(rows.map((r) => r.client_name).filter(Boolean));
    return [...clients]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((c) => ({ value: c, label: c }));
  }, [rows]);

  const filteredReceivedOrders = useMemo(
    () =>
      rows.filter((r) => {
        if (statusFilter && r.status !== statusFilter) return false;
        if (clientFilter && r.client_name !== clientFilter) return false;
        return true;
      }),
    [rows, statusFilter, clientFilter]
  );

  const formatDate = (dateStr: string) => {
    if (!dateStr) return "—";
    return new Date(dateStr).toLocaleDateString("ja-JP");
  };

  const formatAmount = (amount: number | null) => {
    if (amount === null || amount === undefined) return "—";
    return `¥${amount.toLocaleString()}`;
  };

  const isDeletable = (order: ReceivedOrderRow) => order.status === "REGISTERED";

  const deletableRows = useMemo(
    () => filteredReceivedOrders.filter(isDeletable),
    [filteredReceivedOrders]
  );

  const toggleRowSelection = (orderId: number, isDeletable: boolean) => {
    if (!isDeletable) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(orderId)) {
        next.delete(orderId);
      } else {
        next.add(orderId);
      }
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === deletableRows.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(deletableRows.map((r) => r.id)));
    }
  };

  const handleBulkDelete = async () => {
    const count = selectedIds.size;
    if (count === 0) return;

    const msg = `${count}件の受注書を削除します。よろしいですか？`;
    if (!confirm(msg)) return;

    setIsDeleting(true);
    try {
      const results = await Promise.allSettled(
        Array.from(selectedIds).map((id) => deleteReceivedOrder(id))
      );

      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.filter((r) => r.status === "rejected").length;

      qc.invalidateQueries({ queryKey: ["receivedOrders"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setSelectedIds(new Set());

      if (failed === 0) {
        toast.success(`${succeeded}件削除しました`);
      } else {
        toast.error(`${succeeded}件削除、${failed}件は削除できませんでした`);
      }
    } catch (e) {
      toast.error(`削除に失敗しました: ${e instanceof Error ? e.message : e}`);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div className="p-6 space-y-6">
      {selectedIds.size > 0 && (
        <div className="flex items-center gap-3 p-3 bg-blue-500/10 border border-blue-500/30 rounded-lg">
          <span className="text-sm text-blue-400">選択中: {selectedIds.size}件</span>
          <Button
            size="sm"
            variant="outline"
            className="border-red-500/30 text-red-400 hover:text-red-300 hover:border-red-400 ml-auto"
            onClick={handleBulkDelete}
            disabled={isDeleting}
          >
            選択した{selectedIds.size}件を削除
          </Button>
        </div>
      )}
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="w-10">
                  <input
                    type="checkbox"
                    checked={selectedIds.size > 0 && selectedIds.size === deletableRows.length}
                    onChange={toggleSelectAll}
                    disabled={deletableRows.length === 0}
                    className="rounded border-border cursor-pointer disabled:opacity-30 disabled:cursor-default"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">受注番号</TableHead>
                <TableHead className="text-xs text-muted-foreground">対象月</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="クライアント"
                    options={clientOptions}
                    value={clientFilter}
                    onChange={setClientFilter}
                    placeholder="クライアントで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">案件</TableHead>
                <TableHead className="text-xs text-muted-foreground">作業者</TableHead>
                <TableHead className="text-xs text-right">合計金額</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="ステータス"
                    options={statusOptions}
                    value={statusFilter}
                    onChange={setStatusFilter}
                    placeholder="ステータスで検索…"
                  />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableRow><TableCell colSpan={8} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredReceivedOrders.length === 0 ? (
                <TableRow><TableCell colSpan={8} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredReceivedOrders.map((r) => {
                const canDelete = isDeletable(r);
                const isSelected = selectedIds.has(r.id);
                return (
                <TableRow key={r.id} className={`border-border/50 ${canDelete ? "cursor-pointer" : ""} hover:bg-accent/50`}>
                  <TableCell className="w-10" onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleRowSelection(r.id, canDelete)}
                      disabled={!canDelete}
                      className="rounded border-border cursor-pointer disabled:opacity-30 disabled:cursor-default"
                      title={!canDelete ? "受注状態のみ削除できます" : undefined}
                    />
                  </TableCell>
                  <TableCell className="text-sm font-mono text-foreground">
                    <Link href={`/received-orders/${r.id}`} className="hover:underline">
                      {r.received_order_no}
                    </Link>
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(r.target_month)}</TableCell>
                  <TableCell className="text-sm text-foreground">{r.client_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{r.project_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{r.engineer_name}</TableCell>
                  <TableCell className="text-right text-sm tabular-nums">{formatAmount(r.total_amount)}</TableCell>
                  <TableCell><StatusBadge status={r.status} /></TableCell>
                </TableRow>
              );
              })}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredReceivedOrders.length}件</span>
        </div>
      </div>
    </div>
  );
}
