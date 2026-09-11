"use client";

import { Suspense, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchOrders, deleteOrder } from "@/lib/api";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import Link from "next/link";
import { toast } from "sonner";
import type { OrderRow } from "@/lib/types";

export default function OrdersPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <OrdersPageContent />
    </Suspense>
  );
}

function OrdersPageContent() {
  const [statusFilter, setStatusFilter] = useState("");
  const [partnerFilter, setPartnerFilter] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [isDeleting, setIsDeleting] = useState(false);
  const qc = useQueryClient();

  const { data: orders, isLoading } = useQuery({
    queryKey: ["orders", statusFilter, partnerFilter],
    queryFn: () => fetchOrders({
      status: statusFilter || undefined,
      partner: partnerFilter || undefined,
    }),
  });

  const rows = orders ?? [];

  const statusOptions = useMemo(() => {
    const statuses = new Set(rows.map((o) => o.status).filter(Boolean));
    return [...statuses]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((s) => ({ value: s, label: s }));
  }, [rows]);

  const partnerOptions = useMemo(() => {
    const partners = new Set(rows.map((o) => o.partner_name).filter(Boolean));
    return [...partners]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((p) => ({ value: p, label: p }));
  }, [rows]);

  const filteredOrders = useMemo(
    () =>
      rows.filter((o) => {
        if (statusFilter && o.status !== statusFilter) return false;
        if (partnerFilter && o.partner_name !== partnerFilter) return false;
        return true;
      }),
    [rows, statusFilter, partnerFilter]
  );

  const formatDate = (dateStr: string) => {
    if (!dateStr) return "—";
    return new Date(dateStr).toLocaleDateString("ja-JP");
  };

  const formatAmount = (amount: number | null) => {
    if (amount === null || amount === undefined) return "—";
    return `¥${amount.toLocaleString()}`;
  };

  const isDeletable = (order: OrderRow) => order.status === "DRAFT";

  const deletableRows = useMemo(
    () => filteredOrders.filter(isDeletable),
    [filteredOrders]
  );

  const toggleRowSelection = (orderId: string, isDeletable: boolean) => {
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
      setSelectedIds(new Set(deletableRows.map((o) => o.order_id)));
    }
  };

  const handleBulkDelete = async () => {
    const count = selectedIds.size;
    if (count === 0) return;

    const msg = `${count}件の発注書を削除します。よろしいですか？`;
    if (!confirm(msg)) return;

    setIsDeleting(true);
    try {
      const results = await Promise.allSettled(
        Array.from(selectedIds).map((id) => deleteOrder(id))
      );

      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.filter((r) => r.status === "rejected").length;

      qc.invalidateQueries({ queryKey: ["orders"] });
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
                <TableHead className="text-xs text-muted-foreground">発注番号</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="パートナー"
                    options={partnerOptions}
                    value={partnerFilter}
                    onChange={setPartnerFilter}
                    placeholder="パートナーで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">案件</TableHead>
                <TableHead className="text-xs text-muted-foreground">作業者</TableHead>
                <TableHead className="text-xs text-muted-foreground">発注日</TableHead>
                <TableHead className="text-xs text-muted-foreground">作業開始</TableHead>
                <TableHead className="text-xs text-muted-foreground">作業終了</TableHead>
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
                <TableRow><TableCell colSpan={10} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredOrders.length === 0 ? (
                <TableRow><TableCell colSpan={10} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredOrders.map((o) => {
                const canDelete = isDeletable(o);
                const isSelected = selectedIds.has(o.order_id);
                return (
                <TableRow key={o.order_id} className={`border-border/50 ${canDelete ? "cursor-pointer" : ""} hover:bg-accent/50`}>
                  <TableCell className="w-10" onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleRowSelection(o.order_id, canDelete)}
                      disabled={!canDelete}
                      className="rounded border-border cursor-pointer disabled:opacity-30 disabled:cursor-default"
                      title={!canDelete ? "下書き状態のみ削除できます" : undefined}
                    />
                  </TableCell>
                  <TableCell className="text-sm font-mono text-foreground">
                    <Link href={`/orders/${encodeURIComponent(o.order_id)}`} className="hover:underline">
                      {o.order_id}
                    </Link>
                  </TableCell>
                  <TableCell className="text-sm text-foreground">{o.partner_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{o.project_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{o.engineer_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(o.order_date)}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(o.work_start)}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(o.work_end)}</TableCell>
                  <TableCell className="text-right text-sm tabular-nums">{formatAmount(o.total_amount)}</TableCell>
                  <TableCell><StatusBadge status={o.status} /></TableCell>
                </TableRow>
              );
              })}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredOrders.length}件</span>
        </div>
      </div>
    </div>
  );
}
