"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  triggerMailFetch, confirmMail, unconfirmMail,
  fetchImapLockStatus, unlockImap, testImapConnection,
  type MailLog,
} from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";

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

function Empty() {
  return <p className="text-center py-6 text-muted-foreground text-sm">データがありません</p>;
}
