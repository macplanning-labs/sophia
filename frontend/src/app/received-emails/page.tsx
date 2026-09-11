"use client";

import { useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchReceivedEmails, apiPost, apiDelete } from "@/lib/api";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ExternalLink, CheckCircle2, X, Trash2 } from "lucide-react";
import { PageHeader } from "@/components/ui/page-header";
import { DataTableWrapper } from "@/components/ui/data-table-wrapper";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { toast } from "sonner";
import type { ReceivedEmailRow } from "@/lib/types";

const SOURCE_TYPE_LABEL: Record<string, string> = {
  EDI_API: "EDI-OASIS",
  ATTACHMENT: "添付ファイル",
  IGNORED: "対象外",
  UNKNOWN: "不明",
};

const STATUS_BADGE: Record<string, string> = {
  NEW: "bg-slate-500/10 text-slate-400 border-slate-500/30",
  FETCHED: "bg-sky-500/10 text-sky-400 border-sky-500/30",
  IMPORTED: "bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
  FETCH_FAILED: "bg-red-500/10 text-red-400 border-red-500/30",
  PARSE_FAILED: "bg-red-500/10 text-red-400 border-red-500/30",
  DRIVE_FAILED: "bg-amber-500/10 text-amber-400 border-amber-500/30",
};

const STATUS_LABEL: Record<string, string> = {
  NEW: "新規",
  FETCHED: "取得済",
  IMPORTED: "取込済",
  FETCH_FAILED: "取得失敗",
  PARSE_FAILED: "解析失敗",
  DRIVE_FAILED: "Drive失敗",
};

export default function ReceivedEmailsPage() {
  const queryClient = useQueryClient();
  const [needsReviewOnly, setNeedsReviewOnly] = useState(true);
  const [knownDomainOnly, setKnownDomainOnly] = useState(true);
  const [previewEmail, setPreviewEmail] = useState<ReceivedEmailRow | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [sourceFilter, setSourceFilter] = useState("");
  const [fromFilter, setFromFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["received-emails", needsReviewOnly, knownDomainOnly],
    queryFn: () => fetchReceivedEmails(needsReviewOnly, knownDomainOnly),
  });

  const emails = data?.emails ?? [];

  const sourceOptions = useMemo(() => {
    const types = new Set(emails.map((e) => e.source_type).filter(Boolean));
    return [...types]
      .sort((a, b) => a.localeCompare(b))
      .map((t) => ({ value: t, label: SOURCE_TYPE_LABEL[t] ?? t }));
  }, [emails]);

  const fromOptions = useMemo(() => {
    const map = new Map<string, string>();
    for (const e of emails) {
      const key = e.from_email || e.from_name;
      if (!key) continue;
      if (!map.has(key)) map.set(key, e.from_name || e.from_email);
    }
    return [...map.entries()]
      .sort((a, b) => a[1].localeCompare(b[1], "ja"))
      .map(([value, label]) => ({ value, label, searchText: value }));
  }, [emails]);

  const statusOptions = useMemo(() => {
    const present = new Set(emails.map((e) => e.status));
    return [...present]
      .sort((a, b) => a.localeCompare(b))
      .map((s) => ({ value: s, label: STATUS_LABEL[s] ?? s }));
  }, [emails]);

  const filteredEmails = useMemo(
    () =>
      emails.filter((e) => {
        if (sourceFilter && e.source_type !== sourceFilter) return false;
        if (fromFilter && (e.from_email || e.from_name) !== fromFilter) return false;
        if (statusFilter && e.status !== statusFilter) return false;
        return true;
      }),
    [emails, sourceFilter, fromFilter, statusFilter]
  );

  const resolveMutation = useMutation({
    mutationFn: (id: number) => apiPost<{ success: boolean; error?: string }>(`/api/v1/received-emails/${id}/resolve`, {}),
    onSuccess: (res) => {
      if (!res.success) { toast.error(res.error || "処理に失敗しました"); return; }
      queryClient.invalidateQueries({ queryKey: ["received-emails"] });
      toast.success("確認済みにしました");
    },
    onError: (e: Error) => toast.error(`処理に失敗しました: ${e.message}`),
  });

  const deleteMutation = useMutation({
    mutationFn: async (ids: number[]) => {
      await Promise.all(ids.map((id) => apiDelete(`/api/v1/received-emails/${id}`)));
    },
    onSuccess: (_data, ids) => {
      queryClient.invalidateQueries({ queryKey: ["received-emails"] });
      setSelectedIds(new Set());
      setPreviewEmail(null);
      toast.success(`${ids.length}件削除しました`);
    },
    onError: (e: Error) => toast.error(`削除に失敗しました: ${e.message}`),
  });

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    setSelectedIds((prev) =>
      prev.size === filteredEmails.length && filteredEmails.length > 0
        ? new Set()
        : new Set(filteredEmails.map((e) => e.id))
    );
  };

  return (
    <div className="p-6">
      <PageHeader
        actions={
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant={needsReviewOnly ? "default" : "outline"}
              className={needsReviewOnly ? "bg-primary hover:bg-primary/90 text-primary-foreground" : "border-border text-foreground"}
              onClick={() => setNeedsReviewOnly(true)}
            >
              要確認のみ
            </Button>
            <Button
              size="sm"
              variant={!needsReviewOnly ? "default" : "outline"}
              className={!needsReviewOnly ? "bg-primary hover:bg-primary/90 text-primary-foreground" : "border-border text-foreground"}
              onClick={() => setNeedsReviewOnly(false)}
            >
              全件
            </Button>
            <span className="w-px h-5 bg-border mx-1" />
            <Button
              size="sm"
              variant={knownDomainOnly ? "default" : "outline"}
              className={knownDomainOnly ? "bg-primary hover:bg-primary/90 text-primary-foreground" : "border-border text-foreground"}
              onClick={() => setKnownDomainOnly((v) => !v)}
              title="差出人アドレスのドメインが、登録済みのクライアント/パートナーのメールアドレスのいずれかと一致するものだけ表示"
            >
              取引先ドメインのみ
            </Button>
            {selectedIds.size > 0 && (
              <>
                <span className="w-px h-5 bg-border mx-1" />
                <Button
                  size="sm"
                  variant="outline"
                  className="border-red-700/50 text-red-400 hover:text-red-300 hover:border-red-600 gap-1"
                  onClick={() => deleteMutation.mutate(Array.from(selectedIds))}
                  disabled={deleteMutation.isPending}
                >
                  <Trash2 className="w-3.5 h-3.5" /> {selectedIds.size}件を削除
                </Button>
              </>
            )}
          </div>
        }
      />

      <DataTableWrapper
        isLoading={isLoading}
        isEmpty={emails.length === 0}
        emptyMessage={needsReviewOnly ? "現在、要確認のメールはありません" : "受信メールがありません"}
        count={filteredEmails.length}
      >
        <Table>
          <TableHeader>
            <TableRow className="border-border hover:bg-transparent">
              <TableHead className="w-8">
                <input
                  type="checkbox"
                  checked={filteredEmails.length > 0 && selectedIds.size === filteredEmails.length}
                  onChange={toggleSelectAll}
                  onClick={(ev) => ev.stopPropagation()}
                />
              </TableHead>
              <TableHead className="text-xs text-muted-foreground">受信日時</TableHead>
              <TableHead className="text-xs">
                <SearchableColumnHeader
                  label="種別"
                  options={sourceOptions}
                  value={sourceFilter}
                  onChange={setSourceFilter}
                  placeholder="種別で検索…"
                />
              </TableHead>
              <TableHead className="text-xs text-muted-foreground">件名</TableHead>
              <TableHead className="text-xs">
                <SearchableColumnHeader
                  label="差出人"
                  options={fromOptions}
                  value={fromFilter}
                  onChange={setFromFilter}
                  placeholder="差出人で検索…"
                />
              </TableHead>
              <TableHead className="text-xs">
                <SearchableColumnHeader
                  label="ステータス"
                  options={statusOptions}
                  value={statusFilter}
                  onChange={setStatusFilter}
                  placeholder="ステータスで検索…"
                />
              </TableHead>
              <TableHead className="text-xs text-muted-foreground">エラー内容</TableHead>
              <TableHead className="text-xs text-muted-foreground text-center">要確認</TableHead>
              <TableHead className="text-xs text-muted-foreground text-right">アクション</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filteredEmails.length === 0 ? (
              <TableRow>
                <TableCell colSpan={9} className="text-center py-12 text-muted-foreground">
                  条件に一致するデータがありません
                </TableCell>
              </TableRow>
            ) : filteredEmails.map((e) => (
              <TableRow
                key={e.id}
                className="border-border/50 cursor-pointer hover:bg-accent/50"
                onClick={() => setPreviewEmail(e)}
              >
                <TableCell onClick={(ev) => ev.stopPropagation()}>
                  <input type="checkbox" checked={selectedIds.has(e.id)} onChange={() => toggleSelect(e.id)} />
                </TableCell>
                <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                  {e.received_at?.slice(0, 16).replace("T", " ")}
                </TableCell>
                <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                  {SOURCE_TYPE_LABEL[e.source_type] ?? e.source_type}
                </TableCell>
                <TableCell className="text-sm max-w-xs truncate" title={e.subject}>{e.subject || "—"}</TableCell>
                <TableCell className="text-xs text-muted-foreground max-w-[180px] truncate" title={e.from_email}>
                  {e.from_name || e.from_email}
                </TableCell>
                <TableCell>
                  <Badge variant="outline" className={`text-[10px] ${STATUS_BADGE[e.status] ?? "bg-muted text-muted-foreground border-border"}`}>
                    {STATUS_LABEL[e.status] ?? e.status}
                  </Badge>
                </TableCell>
                <TableCell className="text-xs text-red-400 max-w-xs truncate" title={e.error_message}>
                  {e.error_message || "—"}
                </TableCell>
                <TableCell className="text-center">
                  {e.needs_manual_review && <Badge className="bg-amber-500/20 text-amber-400 border-amber-500/30 text-[10px]">要確認</Badge>}
                </TableCell>
                <TableCell className="text-right">
                  <div className="flex items-center justify-end gap-1.5" onClick={(ev) => ev.stopPropagation()}>
                    {e.drive_link && (
                      <a
                        href={e.drive_link}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                        title="Driveで開く"
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                      </a>
                    )}
                    {e.needs_manual_review && (
                      <button
                        onClick={() => resolveMutation.mutate(e.id)}
                        disabled={resolveMutation.isPending}
                        className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
                        title="手動で対応済みにする（既存の受注書・請求書等への反映は別途手動で行ってください）"
                      >
                        <CheckCircle2 className="w-3.5 h-3.5" /> 確認済みにする
                      </button>
                    )}
                    <button
                      onClick={() => deleteMutation.mutate([e.id])}
                      disabled={deleteMutation.isPending}
                      className="p-1.5 rounded-md text-muted-foreground hover:text-red-400 hover:bg-red-500/10 transition-colors disabled:opacity-50"
                      title="このメールを削除（不要と判断した場合）"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </DataTableWrapper>

      {/* プレビューモーダル */}
      {previewEmail && (
        <div
          className="fixed inset-0 z-50 flex items-start justify-center pt-[8vh] bg-black/60 backdrop-blur-sm overflow-y-auto"
          onClick={(e) => { if (e.target === e.currentTarget) setPreviewEmail(null); }}
        >
          <div className="bg-card border border-border rounded-xl shadow-2xl w-full max-w-2xl mx-4 mb-10 animate-in fade-in zoom-in-95 duration-200">
            <div className="flex items-center justify-between px-6 pt-5 pb-3 border-b border-border">
              <h3 className="text-base font-semibold text-foreground truncate pr-4">{previewEmail.subject || "（件名なし）"}</h3>
              <button onClick={() => setPreviewEmail(null)} className="text-muted-foreground hover:text-foreground transition-colors shrink-0">
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="px-6 py-4 space-y-3 max-h-[70vh] overflow-y-auto text-sm">
              <div className="grid grid-cols-2 gap-3 text-xs">
                <div>
                  <dt className="text-muted-foreground">差出人</dt>
                  <dd className="text-foreground">{previewEmail.from_name ? `${previewEmail.from_name} <${previewEmail.from_email}>` : previewEmail.from_email}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">受信日時</dt>
                  <dd className="text-foreground">{previewEmail.received_at?.slice(0, 19).replace("T", " ")}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">種別</dt>
                  <dd className="text-foreground">{SOURCE_TYPE_LABEL[previewEmail.source_type] ?? previewEmail.source_type}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">ステータス</dt>
                  <dd>
                    <Badge variant="outline" className={`text-[10px] ${STATUS_BADGE[previewEmail.status] ?? "bg-muted text-muted-foreground border-border"}`}>
                      {previewEmail.status}
                    </Badge>
                  </dd>
                </div>
                {previewEmail.attachment_filename && (
                  <div className="col-span-2">
                    <dt className="text-muted-foreground">添付ファイル</dt>
                    <dd className="text-foreground">{previewEmail.attachment_filename}</dd>
                  </div>
                )}
              </div>

              {previewEmail.error_message && (
                <div>
                  <dt className="text-xs text-red-400 mb-1">エラー内容</dt>
                  <dd className="text-xs text-red-400 bg-red-500/10 border border-red-500/30 rounded-md p-2 whitespace-pre-wrap">
                    {previewEmail.error_message}
                  </dd>
                </div>
              )}

              {previewEmail.parsed_data != null && (
                <div>
                  <dt className="text-xs text-muted-foreground mb-1">解析結果（parsed_data）</dt>
                  <dd className="text-xs text-foreground bg-muted rounded-md p-2 overflow-x-auto">
                    <pre>{JSON.stringify(previewEmail.parsed_data, null, 2)}</pre>
                  </dd>
                </div>
              )}

              <div>
                <dt className="text-xs text-muted-foreground mb-1">本文</dt>
                <dd className="text-sm text-foreground bg-muted rounded-md p-3 whitespace-pre-wrap max-h-96 overflow-y-auto">
                  {previewEmail.body_text || "（本文なし）"}
                </dd>
              </div>
            </div>
            <div className="flex justify-between px-6 py-4 border-t border-border">
              {previewEmail.drive_link ? (
                <a
                  href={previewEmail.drive_link}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex items-center gap-1 text-xs text-blue-400 hover:text-blue-300"
                >
                  <ExternalLink className="w-3.5 h-3.5" /> Driveで開く
                </a>
              ) : <span />}
              <div className="flex items-center gap-2">
                <button
                  onClick={() => deleteMutation.mutate([previewEmail.id])}
                  disabled={deleteMutation.isPending}
                  className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md bg-red-500/10 text-red-400 border border-red-500/30 hover:bg-red-500/20 transition-colors disabled:opacity-50"
                >
                  <Trash2 className="w-3.5 h-3.5" /> 削除
                </button>
                {previewEmail.needs_manual_review && (
                  <button
                    onClick={() => { resolveMutation.mutate(previewEmail.id); setPreviewEmail(null); }}
                    disabled={resolveMutation.isPending}
                    className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
                  >
                    <CheckCircle2 className="w-3.5 h-3.5" /> 確認済みにする
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
