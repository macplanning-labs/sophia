"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchPeppolTransmissions } from "@/lib/api";
import { PageHeader } from "@/components/ui/page-header";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const STATUS_LABEL: Record<string, { label: string; className: string }> = {
  PENDING:   { label: "処理待ち", className: "border-amber-500/30 text-amber-400" },
  FAILED:    { label: "失敗",     className: "border-red-500/30 text-red-400" },
  UNMATCHED: { label: "未突合",   className: "border-amber-500/30 text-amber-400" },
  SENT:      { label: "送信済",   className: "border-blue-500/30 text-blue-400" },
  ACKED:     { label: "受領確認", className: "border-emerald-500/30 text-emerald-400" },
  RECEIVED:  { label: "受信済",   className: "border-blue-500/30 text-blue-400" },
  MATCHED:   { label: "突合済",   className: "border-emerald-500/30 text-emerald-400" },
};

const DOC_TYPE_LABEL: Record<string, string> = {
  INVOICE: "売上請求書",
  SELF_BILLING: "仕入明細書（セルフビリング）",
};

export default function PeppolTransmissionsPage() {
  // 既定は未処理（FAILED/PENDING/UNMATCHED）のみ表示。feedback: actionableな情報を優先表示。
  const [showAll, setShowAll] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey: ["peppol-transmissions", showAll],
    queryFn: () => fetchPeppolTransmissions(showAll ? "all" : undefined),
  });

  const rows = data ?? [];

  return (
    <div className="space-y-4">
      <PageHeader
        subtitle="Peppol送受信ログ"
        actions={
          <Button variant="outline" size="sm" onClick={() => setShowAll((v) => !v)}>
            {showAll ? "未処理のみ表示" : "すべて表示"}
          </Button>
        }
      />

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        <Table>
          <TableHeader>
            <TableRow className="border-border hover:bg-transparent">
              <TableHead className="text-xs text-muted-foreground">方向</TableHead>
              <TableHead className="text-xs text-muted-foreground">文書種別</TableHead>
              <TableHead className="text-xs text-muted-foreground">関連ID</TableHead>
              <TableHead className="text-xs text-muted-foreground">取引先ID</TableHead>
              <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
              <TableHead className="text-xs text-muted-foreground">日時</TableHead>
              <TableHead className="text-xs text-muted-foreground">エラー</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading && (
              <TableRow><TableCell colSpan={7} className="text-center text-sm text-muted-foreground py-8">読み込み中…</TableCell></TableRow>
            )}
            {!isLoading && rows.length === 0 && (
              <TableRow><TableCell colSpan={7} className="text-center text-sm text-muted-foreground py-8">
                {showAll ? "送受信履歴はありません" : "未処理の送受信はありません"}
              </TableCell></TableRow>
            )}
            {rows.map((row: Record<string, unknown>) => {
              const status = String(row.status ?? "");
              const cfg = STATUS_LABEL[status] ?? { label: status, className: "border-border text-muted-foreground" };
              return (
                <TableRow key={String(row.id)} className="border-border/50">
                  <TableCell className="text-sm">{row.direction === "OUTBOUND" ? "送信" : "受信"}</TableCell>
                  <TableCell className="text-sm">{DOC_TYPE_LABEL[String(row.document_type)] ?? String(row.document_type)}</TableCell>
                  <TableCell className="text-sm">
                    {row.related_id ? (
                      <code className="text-xs">{String(row.related_table)}#{String(row.related_id)}</code>
                    ) : "—"}
                  </TableCell>
                  <TableCell className="text-sm text-muted-foreground">{String(row.participant_id || "—")}</TableCell>
                  <TableCell><Badge variant="outline" className={cn("text-xs", cfg.className)}>{cfg.label}</Badge></TableCell>
                  <TableCell className="text-sm text-muted-foreground">
                    {row.occurred_at ? new Date(String(row.occurred_at)).toLocaleString("ja-JP") : "—"}
                  </TableCell>
                  <TableCell className="text-sm text-red-400 max-w-xs truncate" title={String(row.error_message || "")}>
                    {String(row.error_message || "—")}
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
