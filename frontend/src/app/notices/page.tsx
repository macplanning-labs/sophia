"use client";

import { Suspense, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchNotices, deleteNotice } from "@/lib/api";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import Link from "next/link";
import { toast } from "sonner";
import type { NoticeRow } from "@/lib/types";

export default function NoticesPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <NoticesPageContent />
    </Suspense>
  );
}

function NoticesPageContent() {
  const [partnerFilter, setPartnerFilter] = useState("");
  const [confirmedFilter, setConfirmedFilter] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [isDeleting, setIsDeleting] = useState(false);
  const qc = useQueryClient();

  const { data: notices, isLoading } = useQuery({
    queryKey: ["notices", partnerFilter],
    queryFn: () => fetchNotices({
      partner: partnerFilter || undefined,
    }),
  });

  const rows = notices ?? [];

  const partnerOptions = useMemo(() => {
    const partners = new Set(rows.map((n) => n.partner_name).filter(Boolean));
    return [...partners]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((p) => ({ value: p, label: p }));
  }, [rows]);

  const confirmedOptions = useMemo(() => {
    const opts: { value: string; label: string }[] = [];
    if (rows.some((n) => n.confirmed)) opts.push({ value: "confirmed", label: "確認済" });
    if (rows.some((n) => !n.confirmed)) opts.push({ value: "unconfirmed", label: "未確認" });
    return opts;
  }, [rows]);

  const filteredNotices = useMemo(
    () =>
      rows.filter((n) => {
        if (partnerFilter && n.partner_name !== partnerFilter) return false;
        if (confirmedFilter === "confirmed" && !n.confirmed) return false;
        if (confirmedFilter === "unconfirmed" && n.confirmed) return false;
        return true;
      }),
    [rows, partnerFilter, confirmedFilter]
  );

  const formatDate = (dateStr: string) => {
    if (!dateStr) return "—";
    return new Date(dateStr).toLocaleDateString("ja-JP");
  };

  const formatAmount = (amount: number) => {
    if (amount === null || amount === undefined) return "—";
    return `¥${amount.toLocaleString()}`;
  };

  const isDeletable = (notice: NoticeRow) => !notice.confirmed;

  const deletableRows = useMemo(
    () => filteredNotices.filter(isDeletable),
    [filteredNotices]
  );

  const toggleRowSelection = (noticeId: string, isDeletable: boolean) => {
    if (!isDeletable) return;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(noticeId)) {
        next.delete(noticeId);
      } else {
        next.add(noticeId);
      }
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === deletableRows.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(deletableRows.map((n) => n.notice_id)));
    }
  };

  const handleBulkDelete = async () => {
    const count = selectedIds.size;
    if (count === 0) return;

    const msg = `${count}件の支払通知を削除します。よろしいですか？`;
    if (!confirm(msg)) return;

    setIsDeleting(true);
    try {
      const results = await Promise.allSettled(
        Array.from(selectedIds).map((id) => deleteNotice(id))
      );

      const succeeded = results.filter((r) => r.status === "fulfilled").length;
      const failed = results.filter((r) => r.status === "rejected").length;

      qc.invalidateQueries({ queryKey: ["notices"] });
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
                <TableHead className="text-xs text-muted-foreground">通知番号</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="パートナー"
                    options={partnerOptions}
                    value={partnerFilter}
                    onChange={setPartnerFilter}
                    placeholder="パートナーで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">対象月</TableHead>
                <TableHead className="text-xs text-right">金額</TableHead>
                <TableHead className="text-xs text-muted-foreground">通知日</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="確認状況"
                    options={confirmedOptions}
                    value={confirmedFilter}
                    onChange={setConfirmedFilter}
                    placeholder="確認状況で検索…"
                  />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredNotices.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredNotices.map((n) => {
                const canDelete = isDeletable(n);
                const isSelected = selectedIds.has(n.notice_id);
                return (
                <TableRow key={n.notice_id} className={`border-border/50 ${canDelete ? "cursor-pointer" : ""} hover:bg-accent/50`}>
                  <TableCell className="w-10" onClick={(e) => e.stopPropagation()}>
                    <input
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleRowSelection(n.notice_id, canDelete)}
                      disabled={!canDelete}
                      className="rounded border-border cursor-pointer disabled:opacity-30 disabled:cursor-default"
                      title={!canDelete ? "未確認のみ削除できます" : undefined}
                    />
                  </TableCell>
                  <TableCell className="text-sm font-mono text-foreground">
                    <Link href={`/notices/${encodeURIComponent(n.notice_id)}`} className="hover:underline">
                      {n.notice_id}
                    </Link>
                  </TableCell>
                  <TableCell className="text-sm text-foreground">{n.partner_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(n.target_month)}</TableCell>
                  <TableCell className="text-right text-sm tabular-nums">{formatAmount(n.total)}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{formatDate(n.notice_date)}</TableCell>
                  <TableCell>
                    {n.confirmed ? (
                      <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">確認済</Badge>
                    ) : (
                      <Badge variant="outline" className="border-border text-muted-foreground text-[10px]">未確認</Badge>
                    )}
                  </TableCell>
                </TableRow>
              );
              })}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredNotices.length}件</span>
        </div>
      </div>
    </div>
  );
}
