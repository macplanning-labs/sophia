"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, useCallback } from "react";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { FileText, Receipt, Download, CheckCircle } from "lucide-react";
import { toast } from "sonner";
import { fetchPortalNotices, confirmPortalNotice } from "@/lib/api";

interface Notice {
  notice_id: string;
  project_name: string;
  target_month: string;
  notice_date: string;
  total: number;
  status: string;
  confirmed_at: string | null;
  uuid: string;
}

export default function PortalNoticesPage() {
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pdfType, setPdfType] = useState<"payment-notice" | "invoice">("payment-notice");

  const { data, isLoading } = useQuery({
    queryKey: ["portal-notices"],
    queryFn: fetchPortalNotices,
  });

  const confirmMutation = useMutation({
    mutationFn: confirmPortalNotice,
    onSuccess: (data) => {
      if (data.status === "ok") {
        toast.success(data.message);
        queryClient.invalidateQueries({ queryKey: ["portal-notices"] });
      } else {
        toast.error(data.error || "承諾に失敗しました");
      }
    },
    onError: (err: Error) => toast.error(err.message || "通信エラーが発生しました"),
  });

  const notices: Notice[] = data?.notices ?? [];
  const selected = notices.find((n) => n.notice_id === selectedId);

  const handleConfirm = useCallback(() => {
    if (!selected) return;
    if (!window.confirm(`請求書 ${selected.notice_id} を承諾しますか？\n\nこの操作により、当社が代理作成した請求書を「貴社が発行した請求書」として正式に受理します。\n一度承諾すると取り消しできません。`)) return;
    confirmMutation.mutate(selected.notice_id);
  }, [selected, confirmMutation]);

  const pdfUrl = selected
    ? pdfType === "payment-notice"
      ? `/api/v1/portal/notices/${selected.notice_id}/payment-notice-pdf`
      : `/api/v1/portal/notices/${selected.notice_id}/invoice-pdf`
    : null;

  const statusBadge = (notice: Notice) => {
    if (notice.confirmed_at) {
      return <Badge variant="outline" className="text-[10px] bg-emerald-500/10 text-emerald-400 border-emerald-500/30">承諾済</Badge>;
    }
    return <Badge variant="outline" className="text-[10px] bg-blue-500/10 text-blue-400 border-blue-500/30">送付済</Badge>;
  };

  return (
    <div className="p-6 space-y-4">
      <div>
        <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
          <span className="text-blue-400">📄</span> 請求書一覧
        </h1>
        <p className="text-xs text-muted-foreground mt-1">支払通知書・請求書の確認・承諾ができます</p>
      </div>

      {/* アクションバー */}
      {selected && (
        <div className="bg-card border border-border rounded-lg p-3 flex items-center gap-2 animate-in slide-in-from-top-2 duration-200">
          <span className="text-sm font-medium text-foreground mr-2">{selected.notice_id}</span>
          <button
            onClick={() => setPdfType("payment-notice")}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
              pdfType === "payment-notice" ? "bg-blue-500/10 text-blue-400 border-blue-500/30" : "border-border text-muted-foreground hover:text-foreground"
            )}
          >
            <Receipt className="w-3.5 h-3.5" /> 支払通知書PDF
          </button>
          <button
            onClick={() => setPdfType("invoice")}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
              pdfType === "invoice" ? "bg-blue-500/10 text-blue-400 border-blue-500/30" : "border-border text-muted-foreground hover:text-foreground"
            )}
          >
            <FileText className="w-3.5 h-3.5" /> 請求書PDF
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
            onClick={handleConfirm}
            disabled={selected.confirmed_at !== null || confirmMutation.isPending}
            className={cn(
              "flex items-center gap-1.5 px-4 py-1.5 text-xs font-bold rounded-md transition-all",
              !selected.confirmed_at
                ? "bg-emerald-600 text-white hover:bg-emerald-500 shadow-lg shadow-emerald-600/20 active:scale-95"
                : "bg-muted text-muted-foreground cursor-not-allowed"
            )}
          >
            <CheckCircle className="w-3.5 h-3.5" />
            {confirmMutation.isPending ? "処理中..." : !selected.confirmed_at ? "承諾する" : "承諾済"}
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
                <TableHead className="text-xs text-muted-foreground">請求書番号</TableHead>
                <TableHead className="text-xs text-muted-foreground">案件</TableHead>
                <TableHead className="text-xs text-muted-foreground">対象年月</TableHead>
                <TableHead className="text-xs text-muted-foreground">請求日</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">金額(税込)</TableHead>
                <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {notices.length === 0 ? (
                <TableRow><TableCell colSpan={6} className="text-center py-12 text-muted-foreground">請求書がありません</TableCell></TableRow>
              ) : notices.map((notice) => (
                <TableRow
                  key={notice.notice_id}
                  onClick={() => setSelectedId(notice.notice_id === selectedId ? null : notice.notice_id)}
                  className={cn(
                    "border-border/50 cursor-pointer transition-colors",
                    notice.notice_id === selectedId
                      ? "bg-primary/5 border-l-2 border-l-primary"
                      : "hover:bg-accent/50"
                  )}
                >
                  <TableCell className="text-sm font-mono font-medium">{notice.notice_id}</TableCell>
                  <TableCell className="text-sm">{notice.project_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{notice.target_month}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{notice.notice_date}</TableCell>
                  <TableCell className="text-sm text-right tabular-nums">¥{notice.total.toLocaleString()}</TableCell>
                  <TableCell>{statusBadge(notice)}</TableCell>
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
              {pdfType === "payment-notice" ? "支払通知書" : "請求書"} — {selected.notice_id}
            </p>
          </div>
          <iframe src={pdfUrl} className="w-full h-[600px]" title="PDF Preview" />
        </div>
      )}
    </div>
  );
}
