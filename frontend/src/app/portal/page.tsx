"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, useCallback } from "react";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { FileText, FileCheck, Download, CheckCircle } from "lucide-react";
import { toast } from "sonner";
import { fetchPortalOrders, approvePortalOrder } from "@/lib/api";

interface Order {
  order_id: string;
  project_name: string;
  engineer_name: string;
  target_month: string;
  order_date: string;
  total_amount: number;
  status: string;
  uuid: string;
}

export default function PortalOrdersPage() {
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pdfType, setPdfType] = useState<"order" | "acceptance">("order");

  const { data, isLoading } = useQuery({
    queryKey: ["portal-orders"],
    queryFn: fetchPortalOrders,
  });

  const approveMutation = useMutation({
    mutationFn: approvePortalOrder,
    onSuccess: (data) => {
      if (data.status === "ok") {
        toast.success(data.message);
        queryClient.invalidateQueries({ queryKey: ["portal-orders"] });
      } else {
        toast.error(data.error || "承諾に失敗しました");
      }
    },
    onError: (err: Error) => toast.error(err.message || "通信エラーが発生しました"),
  });

  const orders: Order[] = data?.orders ?? [];
  const selected = orders.find((o) => o.order_id === selectedId);

  const handleApprove = useCallback(() => {
    if (!selected) return;
    if (!window.confirm(`注文書 ${selected.order_id} を承諾しますか？\n\n一度承諾すると取り消しできません。`)) return;
    approveMutation.mutate(selected.order_id);
  }, [selected, approveMutation]);

  const pdfUrl = selected
    ? pdfType === "order"
      ? `/api/v1/portal/orders/${selected.order_id}/pdf`
      : `/api/v1/portal/orders/${selected.order_id}/acceptance-pdf`
    : null;

  const statusBadge = (status: string) => {
    const map: Record<string, { label: string; className: string }> = {
      SENT: { label: "送付済", className: "bg-blue-500/10 text-blue-400 border-blue-500/30" },
      ACCEPTED: { label: "承諾済", className: "bg-emerald-500/10 text-emerald-400 border-emerald-500/30" },
      NOTICE_CONFIRMED: { label: "請求承諾済", className: "bg-purple-500/10 text-purple-400 border-purple-500/30" },
    };
    const s = map[status] || { label: status, className: "bg-muted text-muted-foreground border-border" };
    return <Badge variant="outline" className={cn("text-[10px]", s.className)}>{s.label}</Badge>;
  };

  return (
    <div className="p-6 space-y-4">
      <div>
        <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
          <span className="text-blue-400">📋</span> 注文書一覧
        </h1>
        <p className="text-xs text-muted-foreground mt-1">弊社からの注文書を確認・承諾できます</p>
      </div>

      {/* アクションバー（選択時のみ表示） */}
      {selected && (
        <div className="bg-card border border-border rounded-lg p-3 flex items-center gap-2 animate-in slide-in-from-top-2 duration-200">
          <span className="text-sm font-medium text-foreground mr-2">{selected.order_id}</span>
          <button
            onClick={() => setPdfType("order")}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
              pdfType === "order" ? "bg-blue-500/10 text-blue-400 border-blue-500/30" : "border-border text-muted-foreground hover:text-foreground"
            )}
          >
            <FileText className="w-3.5 h-3.5" /> 注文書PDF
          </button>
          <button
            onClick={() => setPdfType("acceptance")}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
              pdfType === "acceptance" ? "bg-blue-500/10 text-blue-400 border-blue-500/30" : "border-border text-muted-foreground hover:text-foreground"
            )}
          >
            <FileCheck className="w-3.5 h-3.5" /> 注文請書PDF
          </button>
          <a
            href={pdfUrl || "#"}
            download
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-border text-muted-foreground hover:text-foreground transition-colors"
          >
            <Download className="w-3.5 h-3.5" /> ダウンロード
          </a>
          <div className="flex-1" />
          <button
            onClick={handleApprove}
            disabled={selected.status !== "SENT" || approveMutation.isPending}
            className={cn(
              "flex items-center gap-1.5 px-4 py-1.5 text-xs font-bold rounded-md transition-all",
              selected.status === "SENT"
                ? "bg-emerald-600 text-white hover:bg-emerald-500 shadow-lg shadow-emerald-600/20 active:scale-95"
                : "bg-muted text-muted-foreground cursor-not-allowed"
            )}
          >
            <CheckCircle className="w-3.5 h-3.5" />
            {approveMutation.isPending ? "処理中..." : selected.status === "SENT" ? "承諾する" : "承諾済"}
          </button>
        </div>
      )}

      {/* テーブル */}
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">注文書番号</TableHead>
                <TableHead className="text-xs text-muted-foreground">案件</TableHead>
                <TableHead className="text-xs text-muted-foreground">技術者</TableHead>
                <TableHead className="text-xs text-muted-foreground">対象年月</TableHead>
                <TableHead className="text-xs text-muted-foreground">受注日</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">金額(税抜)</TableHead>
                <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orders.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">注文書がありません</TableCell></TableRow>
              ) : orders.map((order) => (
                <TableRow
                  key={order.order_id}
                  onClick={() => setSelectedId(order.order_id === selectedId ? null : order.order_id)}
                  className={cn(
                    "border-border/50 cursor-pointer transition-colors",
                    order.order_id === selectedId
                      ? "bg-primary/5 border-l-2 border-l-primary"
                      : "hover:bg-accent/50"
                  )}
                >
                  <TableCell className="text-sm font-mono font-medium">{order.order_id}</TableCell>
                  <TableCell className="text-sm">{order.project_name}</TableCell>
                  <TableCell className="text-sm">{order.engineer_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{order.target_month}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{order.order_date}</TableCell>
                  <TableCell className="text-sm text-right tabular-nums">¥{order.total_amount.toLocaleString()}</TableCell>
                  <TableCell>{statusBadge(order.status)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </div>

      {/* PDFプレビュー */}
      {selected && pdfUrl && (
        <div className="bg-card border border-border rounded-lg overflow-hidden">
          <div className="px-4 py-2 border-b border-border bg-muted/30">
            <p className="text-xs text-muted-foreground">
              {pdfType === "order" ? "注文書" : "注文請書"} — {selected.order_id}
            </p>
          </div>
          <iframe src={pdfUrl} className="w-full h-[600px]" title="PDF Preview" />
        </div>
      )}
    </div>
  );
}
