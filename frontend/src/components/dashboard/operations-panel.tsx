"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  fetchEdiOrders, importEdiOrder, fetchEdiInvoices, approveEdiInvoice,
  triggerImportAll, triggerMailFetch, confirmMail, unconfirmMail,
  fetchImapLockStatus, unlockImap, testImapConnection,
  type EdiOrder, type EdiInvoice, type MailLog,
} from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";

// ── EDI操作パネル ──

export function EdiPanel() {
  const queryClient = useQueryClient();
  const now = new Date();
  const [year, setYear] = useState(now.getFullYear());
  const [month, setMonth] = useState(now.getMonth() + 1);
  const [tab, setTab] = useState<"orders" | "invoices">("orders");

  const { data: orderData, isLoading: ordersLoading } = useQuery({
    queryKey: ["edi-orders", year, month],
    queryFn: () => fetchEdiOrders(year, month),
    enabled: tab === "orders",
  });

  const { data: invoiceData, isLoading: invoicesLoading } = useQuery({
    queryKey: ["edi-invoices", year, month],
    queryFn: () => fetchEdiInvoices(year, month),
    enabled: tab === "invoices",
  });

  const importMutation = useMutation({
    mutationFn: importEdiOrder,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["edi-orders"] }); toast.success("EDI注文書を取り込みました"); },
    onError: (e: Error) => toast.error(`EDI取込エラー: ${e.message}`),
  });

  const approveMutation = useMutation({
    mutationFn: approveEdiInvoice,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["edi-invoices"] }); toast.success("EDI請求書を承認しました"); },
    onError: (e: Error) => toast.error(`EDI請求書承認エラー: ${e.message}`),
  });

  const importAllMutation = useMutation({
    mutationFn: () => triggerImportAll(year, month),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ["edi-orders"] });
      queryClient.invalidateQueries({ queryKey: ["edi-invoices"] });
      toast.success(data?.error ?? "一括取込が完了しました（PDF生成はバックグラウンドで実行中）");
    },
    onError: (e: Error) => toast.error(`一括取込エラー: ${e.message}`),
  });

  return (
    <div className="bg-card border border-border rounded-lg">
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold text-foreground">🔗 EDI-OASIS</h2>
          <div className="flex gap-1">
            <button onClick={() => setTab("orders")} className={cn("px-2 py-1 text-xs rounded", tab === "orders" ? "bg-muted text-foreground" : "text-muted-foreground hover:text-foreground")}>注文書</button>
            <button onClick={() => setTab("invoices")} className={cn("px-2 py-1 text-xs rounded", tab === "invoices" ? "bg-muted text-foreground" : "text-muted-foreground hover:text-foreground")}>請求書</button>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <select className="bg-muted border border-border rounded px-2 py-1 text-xs text-foreground" value={`${year}-${month}`}
            onChange={(e) => { const [y, m] = e.target.value.split("-"); setYear(Number(y)); setMonth(Number(m)); }}>
            {Array.from({ length: 6 }, (_, i) => {
              const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
              return <option key={i} value={`${d.getFullYear()}-${d.getMonth() + 1}`}>{d.getFullYear()}年{d.getMonth() + 1}月</option>;
            })}
          </select>
          <button onClick={() => importAllMutation.mutate()} disabled={importAllMutation.isPending}
            className="px-2 py-1 bg-primary hover:bg-primary/80 disabled:opacity-40 text-primary-foreground text-xs rounded transition-colors">
            {importAllMutation.isPending ? "取込中..." : "一括取込"}
          </button>
        </div>
      </div>
      <div className="p-3 max-h-64 overflow-y-auto">
        {tab === "orders" ? (
          ordersLoading ? <Skeleton /> : (
            <div className="space-y-1">
              {(orderData?.orders ?? []).length === 0 ? <Empty /> : orderData?.orders.map((o) => (
                <div key={o.id} className="flex items-center justify-between px-3 py-2 rounded hover:bg-accent/50 text-sm">
                  <div className="flex items-center gap-3">
                    <span className="text-muted-foreground text-xs w-20">{o.order_no}</span>
                    <span className="text-foreground">{o.project_name}</span>
                    <span className="text-muted-foreground text-xs">{o.worker_name}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs tabular-nums text-muted-foreground">¥{o.amount?.toLocaleString()}</span>
                    {o.is_imported ? (
                      <Badge variant="outline" className="text-[10px] bg-emerald-500/20 text-emerald-400 border-emerald-500/30">取込済</Badge>
                    ) : (
                      <button onClick={() => importMutation.mutate(o.id)} disabled={importMutation.isPending}
                        className="px-2 py-0.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-foreground text-[10px] rounded transition-colors">取込</button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )
        ) : (
          invoicesLoading ? <Skeleton /> : (
            <div className="space-y-1">
              {(invoiceData?.invoices ?? []).length === 0 ? <Empty /> : invoiceData?.invoices.map((inv) => (
                <div key={inv.id} className="flex items-center justify-between px-3 py-2 rounded hover:bg-accent/50 text-sm">
                  <div className="flex items-center gap-3">
                    <span className="text-muted-foreground text-xs w-20">{inv.invoice_no}</span>
                    <span className="text-foreground">{inv.project_name}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs tabular-nums text-muted-foreground">¥{inv.amount?.toLocaleString()}</span>
                    {inv.is_approved ? (
                      <Badge variant="outline" className="text-[10px] bg-emerald-500/20 text-emerald-400 border-emerald-500/30">承認済</Badge>
                    ) : (
                      <button onClick={() => approveMutation.mutate(inv.id)} disabled={approveMutation.isPending}
                        className="px-2 py-0.5 bg-amber-600 hover:bg-amber-500 disabled:opacity-40 text-foreground text-[10px] rounded transition-colors">承認</button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )
        )}
      </div>
    </div>
  );
}

// ── メール操作パネル ──

export function MailPanel({ mailLogs }: { mailLogs: MailLog[] }) {
  const queryClient = useQueryClient();
  const { isAdmin } = useCurrentUser();

  const fetchMutation = useMutation({
    mutationFn: triggerMailFetch,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["dashboard"] }); toast.success("メールチェック完了"); },
    onError: (e: Error) => toast.error(`メールチェックエラー: ${e.message}`),
  });

  // IMAPサーキットブレイカーの状態表示・解除（管理者専用APIのため isAdmin の場合のみ問い合わせる）
  const { data: lockStatus } = useQuery({
    queryKey: ["imap-lock-status"],
    queryFn: fetchImapLockStatus,
    enabled: isAdmin,
    refetchInterval: 60_000,
  });

  const unlockMutation = useMutation({
    mutationFn: unlockImap,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["imap-lock-status"] }); toast.success("IMAPロックを解除しました"); },
    onError: (e: Error) => toast.error(`ロック解除エラー: ${e.message}`),
  });

  const imapTestMutation = useMutation({
    mutationFn: testImapConnection,
    onSuccess: (res) => {
      if (res.success) toast.success("IMAP接続確認: 成功しました");
      else toast.error(`IMAP接続確認: 失敗（${res.error || "不明なエラー"}）`);
    },
    onError: (e: Error) => toast.error(`IMAP接続確認エラー: ${e.message}`),
  });

  const confirmMutation = useMutation({
    mutationFn: confirmMail,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["dashboard"] }); toast.success("確認済みにしました"); },
    onError: (e: Error) => toast.error(`確認処理エラー: ${e.message}`),
  });

  const unconfirmMutation = useMutation({
    mutationFn: unconfirmMail,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["dashboard"] }); toast.info("確認を取り消しました"); },
    onError: (e: Error) => toast.error(`取消処理エラー: ${e.message}`),
  });

  const classificationLabel: Record<string, string> = {
    ORDER: "注文書",
    INVOICE: "請求書",
    REPORT: "報告書",
    OTHER: "その他",
  };

  return (
    <div className="bg-card border border-border rounded-lg">
      <div className="px-4 py-3 border-b border-border flex items-center justify-between">
        <h2 className="text-sm font-semibold text-foreground">📧 メール受信ログ</h2>
        <div className="flex items-center gap-2">
          {isAdmin && (
            <button onClick={() => imapTestMutation.mutate()} disabled={imapTestMutation.isPending}
              className="px-2 py-1 bg-muted hover:bg-muted disabled:opacity-40 text-foreground text-xs rounded transition-colors">
              {imapTestMutation.isPending ? "確認中..." : "IMAP接続確認"}
            </button>
          )}
          <button onClick={() => fetchMutation.mutate()} disabled={fetchMutation.isPending}
            className="px-2 py-1 bg-muted hover:bg-muted disabled:opacity-40 text-foreground text-xs rounded transition-colors">
            {fetchMutation.isPending ? "取得中..." : "メールチェック"}
          </button>
        </div>
      </div>
      {lockStatus?.locked && (
        <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/30 flex items-center justify-between gap-3">
          <p className="text-xs text-destructive">
            🔒 IMAPアクセスは連続認証失敗によりロック中です{lockStatus.reason ? `（${lockStatus.reason}）` : ""}。
            認証情報を確認してから解除してください。
          </p>
          <button onClick={() => unlockMutation.mutate()} disabled={unlockMutation.isPending}
            className="px-2 py-1 bg-destructive hover:bg-destructive/80 disabled:opacity-40 text-destructive-foreground text-[10px] rounded transition-colors shrink-0">
            {unlockMutation.isPending ? "解除中..." : "ロック解除"}
          </button>
        </div>
      )}
      <div className="p-3 max-h-48 overflow-y-auto">
        {mailLogs.length === 0 ? <Empty /> : (
          <div className="space-y-1">
            {mailLogs.slice(0, 20).map((m) => (
              <div key={m.id} className="flex items-center justify-between px-3 py-2 rounded hover:bg-accent/50 text-sm">
                <div className="flex items-center gap-3 min-w-0">
                  <Badge variant="outline" className="text-[10px] border-border shrink-0">{classificationLabel[m.classification] ?? m.classification}</Badge>
                  <span className="text-muted-foreground text-xs shrink-0">{m.sender_name}</span>
                  <span className="text-foreground truncate">{m.subject}</span>
                </div>
                <div className="flex items-center gap-2 shrink-0 ml-2">
                  <span className="text-[10px] text-muted-foreground">{m.received_at?.slice(0, 10)}</span>
                  {m.is_reflected ? (
                    <button onClick={() => unconfirmMutation.mutate(m.id)} className="text-[10px] text-emerald-400 hover:text-muted-foreground transition-colors">✓確認済</button>
                  ) : (
                    <button onClick={() => confirmMutation.mutate(m.id)} className="text-[10px] text-muted-foreground hover:text-emerald-400 transition-colors">未確認</button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function Skeleton() {
  return <div className="space-y-2 p-2">{[...Array(3)].map((_, i) => <div key={i} className="h-8 bg-muted/50 rounded animate-pulse" />)}</div>;
}

function Empty() {
  return <p className="text-center py-6 text-muted-foreground text-sm">データがありません</p>;
}
