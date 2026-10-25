"use client";

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Settings } from "lucide-react";
import { fetchSentEmail, fetchSentEmails } from "@/lib/api";
import { filterSentEmails, formatSentAt } from "@/lib/sent-emails";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/ui/page-header";
import { DataTableWrapper } from "@/components/ui/data-table-wrapper";
import { DetailModal } from "@/components/ui/detail-modal";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { AccessDenied } from "@/components/access-denied";
import { MailAccountDialog } from "@/components/mail/mail-account-dialog";

/** メール > 送信: Sophia が送ったメールの記録(h_sent_email)。右上の ⚙ でメールアカウントを設定する(DEMO-000196) */
export default function SentEmailsPage() {
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const [query, setQuery] = useState("");
  const [openId, setOpenId] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey: ["sent-emails"],
    queryFn: fetchSentEmails,
    enabled: isAdmin,
  });
  const emails = useMemo(() => data?.emails ?? [], [data]);
  const filtered = useMemo(() => filterSentEmails(emails, query), [emails, query]);

  if (userLoading) return null;
  if (!isAdmin) return <AccessDenied message="送信メールを見るには管理者権限が必要です。" />;

  return (
    <div className="p-6">
      <PageHeader
        actions={
          <div className="flex items-center gap-2">
            <input
              className="w-64 px-3 py-1.5 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="宛先・件名で検索…"
            />
            <Button size="sm" variant="outline" className="border-border text-foreground gap-1" onClick={() => setSettingsOpen(true)} title="メールアカウントの設定">
              <Settings className="w-3.5 h-3.5" /> 設定
            </Button>
          </div>
        }
      />

      <DataTableWrapper
        isLoading={isLoading}
        isEmpty={emails.length === 0}
        emptyMessage="送信したメールの記録はありません"
        count={filtered.length}
      >
        <Table>
          <TableHeader>
            <TableRow className="border-border hover:bg-transparent">
              <TableHead className="text-xs text-muted-foreground w-40">送信日時</TableHead>
              <TableHead className="text-xs text-muted-foreground">宛先</TableHead>
              <TableHead className="text-xs text-muted-foreground">件名</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filtered.map((e) => (
              <TableRow key={e.id} className="border-border/50 hover:bg-accent/50 cursor-pointer" onClick={() => setOpenId(e.id)}>
                <TableCell className="text-sm text-muted-foreground whitespace-nowrap">{formatSentAt(e.sent_at)}</TableCell>
                <TableCell className="text-sm">{e.recipient || "—"}</TableCell>
                <TableCell className="text-sm">{e.subject}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </DataTableWrapper>
      {data && emails.length >= data.limit && (
        <p className="text-[11px] text-muted-foreground mt-2">新しい順に {data.limit} 件まで表示しています。</p>
      )}

      {openId !== null && <SentEmailModal id={openId} onClose={() => setOpenId(null)} />}
      <MailAccountDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}

function SentEmailModal({ id, onClose }: { id: number; onClose: () => void }) {
  const { data, isLoading } = useQuery({ queryKey: ["sent-email", id], queryFn: () => fetchSentEmail(id) });
  const m = data?.email;
  return (
    <DetailModal open title={m?.subject ?? "送信メール"} size="lg" onClose={onClose}>
      {isLoading || !m ? (
        <div className="h-32 bg-muted rounded-lg animate-pulse" />
      ) : (
        <div className="space-y-4">
          <dl className="grid grid-cols-[6rem_1fr] gap-y-1 text-sm">
            <dt className="text-muted-foreground">送信日時</dt>
            <dd className="text-foreground">{formatSentAt(m.sent_at)}</dd>
            <dt className="text-muted-foreground">宛先</dt>
            <dd className="text-foreground">{m.recipient || "—"}</dd>
          </dl>
          <pre className="whitespace-pre-wrap break-words text-sm text-foreground bg-muted/50 border border-border rounded-lg p-4 font-sans">{m.body}</pre>
        </div>
      )}
    </DetailModal>
  );
}
