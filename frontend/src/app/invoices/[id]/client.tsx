"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchInvoiceDetail, fetchInvoiceEmailPreview, approveInvoice, rejectInvoice, sendInvoiceMail, sendInvoicePeppol, downloadBlob, apiPut } from "@/lib/api";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { StatusBadge } from "@/components/ui/status-badge";
import { FormModal, FormField, FormInput, FormTextarea } from "@/components/ui/form-modal";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FileText, Download, X } from "lucide-react";
import { toast } from "sonner";
import { useDynamicId } from "@/lib/utils";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { GuidanceCallout } from "@/components/ui/guidance-callout";
import { getInvoiceStatusGuidance } from "@/lib/guidance/orders";

function itemAdjustment(item: Record<string, unknown>): number {
  if (item.adjustment != null && item.adjustment !== "") {
    return Number(item.adjustment);
  }
  return Number(item.amount ?? 0) - Number(item.unit_price ?? 0);
}

export default function InvoiceDetailPage({
  invoiceId,
  embedded = false,
  onDeleted,
}: {
  invoiceId?: string;
  embedded?: boolean;
  onDeleted?: () => void;
} = {}) {
  const routeId = useDynamicId();
  const id = invoiceId || routeId;
  const router = useRouter();
  const qc = useQueryClient();
  const { isAdmin } = useCurrentUser();
  const { data, isLoading } = useQuery({
    queryKey: ["invoices", id],
    queryFn: () => fetchInvoiceDetail(id),
    enabled: !!id,
  });

  const deleteInvoice = useMutation({
    mutationFn: () => fetch(`/api/v1/invoices/${id}/delete`, { method: "POST" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "削除に失敗しました"); } return r; }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invoices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      if (onDeleted) onDeleted();
      else if (!embedded) router.push("/");
    },
    onError: (e: Error) => toast.error(`削除に失敗しました: ${e.message}`),
  });

  const approveMut = useMutation({
    mutationFn: () => approveInvoice(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invoices", id] });
      qc.invalidateQueries({ queryKey: ["invoices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success("承認しました");
    },
    onError: (e: Error) => toast.error(`承認に失敗しました: ${e.message}`),
  });

  const rejectMut = useMutation({
    mutationFn: () => rejectInvoice(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invoices", id] });
      qc.invalidateQueries({ queryKey: ["invoices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success("差戻ししました");
    },
    onError: (e: Error) => toast.error(`差戻しに失敗しました: ${e.message}`),
  });

  const peppolSendMut = useMutation({
    mutationFn: () => sendInvoicePeppol(id),
    onSuccess: (res) => {
      if (res.success === false) { toast.error(`Peppol送信に失敗しました: ${res.error}`); return; }
      toast.success(`Peppol経由で送信しました（メッセージID: ${res.message_id}）`);
    },
    onError: (e: Error) => toast.error(`Peppol送信に失敗しました: ${e.message}`),
  });

  // ── 編集モーダル ──
  const [editOpen, setEditOpen] = useState(false);
  const [editForm, setEditForm] = useState({
    issue_date: "",
    due_date: "",
    subject: "",
    notes: "",
  });

  const editMutation = useMutation({
    mutationFn: () => apiPut(`/api/v1/invoices/${id}`, editForm),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["invoices", id] });
      qc.invalidateQueries({ queryKey: ["invoices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setEditOpen(false);
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(`更新に失敗しました: ${e.message}`),
  });

  const openEditModal = () => {
    if (inv) {
      setEditForm({
        issue_date: inv.issue_date || "",
        due_date: inv.due_date || "",
        subject: inv.subject || "",
        notes: data?.notes || "",
      });
      setEditOpen(true);
    }
  };

  // ── PDFプレビュー（詳細下に表示。メール送信中も確認可能） ──
  const [pdfOpen, setPdfOpen] = useState(false);
  const pdfUrl = pdfOpen ? `/api/v1/invoices/${id}/pdf` : null;

  const handlePdfDownload = async () => {
    if (!pdfUrl) return;
    try {
      const res = await fetch(pdfUrl, { credentials: "include" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const blob = await res.blob();
      const filename = `invoice_${data?.invoice?.invoice_id ?? id}.pdf`;
      downloadBlob(blob, filename);
    } catch (e) {
      toast.error(`PDFダウンロードに失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };

  // ── 送信モーダル ──
  const [sendOpen, setSendOpen] = useState(false);
  const [emailForm, setEmailForm] = useState({ subject: "", body: "" });
  const [sendMeta, setSendMeta] = useState<{ to_email?: string | null; cc_email?: string | null; client_name?: string }>({});

  const openSendModal = async () => {
    // 送信前にPDFを画面内で確認できるようプレビューを開く
    setPdfOpen(true);
    try {
      const preview = await fetchInvoiceEmailPreview(id);
      setEmailForm({ subject: preview.subject, body: preview.body });
      setSendMeta({
        to_email: preview.to_email,
        cc_email: preview.cc_email,
        client_name: preview.client_name,
      });
      setSendOpen(true);
    } catch (e) {
      toast.error(`メール本文の取得に失敗しました: ${(e as Error).message}`);
    }
  };

  const sendMut = useMutation({
    mutationFn: () => sendInvoiceMail(id, emailForm),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["invoices", id] });
      qc.invalidateQueries({ queryKey: ["invoices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success(res.message || "請求書ダウンロード用URLをメール送信しました");
      setSendOpen(false);
      // PDFプレビューは開いたまま（支払通知詳細と同じ）
    },
    onError: (e: Error) => toast.error(`送信に失敗しました: ${e.message}`),
  });

  const inv = data?.invoice;
  const items = data?.items ?? [];
  const status = inv?.status;

  const actions = (
    <div className="flex items-center gap-2">
      <button
        type="button"
        onClick={() => setPdfOpen(true)}
        className="px-3 py-1.5 text-xs font-medium rounded-md bg-purple-500/10 text-purple-400 border border-purple-500/30 hover:bg-purple-500/20 transition-colors inline-flex items-center gap-1"
      >
        <FileText className="w-3.5 h-3.5" /> PDF
      </button>
      {isAdmin && status === "PENDING_APPROVAL" && (
        <button
          onClick={() => approveMut.mutate()}
          disabled={approveMut.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
        >
          ✅ 承認
        </button>
      )}
      {isAdmin && status === "APPROVED" && (
        <button
          onClick={() => { if (confirm("差戻ししますか？")) rejectMut.mutate(); }}
          disabled={rejectMut.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-red-500/10 text-red-400 border border-red-500/30 hover:bg-red-500/20 transition-colors disabled:opacity-50"
        >
          ↩ 差戻し
        </button>
      )}
      {isAdmin && (status === "APPROVED" || status === "SENT") && (
        <button
          onClick={openSendModal}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-blue-500/10 text-blue-400 border border-blue-500/30 hover:bg-blue-500/20 transition-colors disabled:opacity-50"
        >
          {status === "SENT" ? "🔁 再送信" : "📧 送信メール確認・送信"}
        </button>
      )}
      {isAdmin && (status === "APPROVED" || status === "SENT") && (
        <button
          onClick={() => peppolSendMut.mutate()}
          disabled={peppolSendMut.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 hover:bg-indigo-500/20 transition-colors disabled:opacity-50"
        >
          🌐 Peppol送信
        </button>
      )}
      {isAdmin && !inv?.client_accepted_at && (
        <button
          onClick={openEditModal}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 hover:bg-indigo-500/20 transition-colors"
        >
          ✏️ 編集
        </button>
      )}
      {isAdmin && !inv?.client_accepted_at && (
        <button
          onClick={() => { if (confirm("この請求書を削除しますか？")) deleteInvoice.mutate(); }}
          disabled={deleteInvoice.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-red-500/10 text-red-400 border border-red-500/30 hover:bg-red-500/20 transition-colors disabled:opacity-50"
        >
          🗑 削除
        </button>
      )}
    </div>
  );

  return (
    <DetailLayout title={`請求書 #${id}`} icon="📑" backHref="/" backLabel="ダッシュボードに戻る" isLoading={isLoading} actions={actions} embedded={embedded}>
      {inv && (
        <>
          {(() => {
            const g = getInvoiceStatusGuidance(status ?? "");
            return g ? (
              <div className="mb-4">
                <GuidanceCallout variant="info" {...g} />
              </div>
            ) : null;
          })()}
          <FieldGrid>
            <Field label="請求書番号" value={inv.invoice_id} />
            <Field label="クライアント" value={data.client_name} />
            <Field label="案件名" value={data.project_name || "—"} />
            <Field label="件名" value={inv.subject} />
          </FieldGrid>
          <FieldGrid>
            <Field label="発行日" value={inv.issue_date} />
            <Field label="支払期日" value={inv.due_date || "—"} />
            <Field label="ステータス" value={<StatusBadge status={inv.status} />} />
            <Field label="受注書" value={
              data.received_order_no
                ? (embedded ? data.received_order_no : (
                  <a href={`/received-orders/${data.received_order_no}`} className="text-blue-400 hover:underline">{data.received_order_no}</a>
                ))
                : "—"
            } />
          </FieldGrid>
          <FieldGrid>
            <Field label="クライアント受領確認" value={
              inv.client_accepted_at
                ? `確認済み（${new Date(inv.client_accepted_at).toLocaleString("ja-JP")}）`
                : "未確認"
            } />
          </FieldGrid>
          {(inv.approved_at || inv.sent_at) && (
            <FieldGrid cols={2}>
              {inv.approved_at && <Field label="承認日時" value={new Date(inv.approved_at).toLocaleString("ja-JP")} />}
              {inv.sent_at && <Field label="送信日時" value={new Date(inv.sent_at).toLocaleString("ja-JP")} />}
            </FieldGrid>
          )}
          <FieldGrid cols={3}>
            <Field label="小計" value={`¥${Number(data.total_amount).toLocaleString()}`} />
            <Field label="消費税（10%）" value={`¥${Number(data.tax_amount).toLocaleString()}`} />
            <Field label="合計" value={
              <span className="text-lg font-bold text-emerald-400">¥{Number(data.grand_total).toLocaleString()}</span>
            } />
          </FieldGrid>

          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-border">
              <span className="text-sm font-medium text-foreground">請求明細 ({items.length}件)</span>
            </div>
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">品目</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">数量</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">単価</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">調整金</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">金額</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item: Record<string, unknown>, i: number) => {
                  const adj = itemAdjustment(item);
                  return (
                    <TableRow key={i} className="border-border/50">
                      <TableCell className="text-sm">{String(item.product_name ?? item.description ?? "—")}</TableCell>
                      <TableCell className="text-right text-sm tabular-nums">{String(item.man_month ?? item.quantity ?? "—")}</TableCell>
                      <TableCell className="text-right text-sm tabular-nums">¥{Number(item.unit_price ?? 0).toLocaleString()}</TableCell>
                      <TableCell className={`text-right text-sm tabular-nums ${adj < 0 ? "text-red-400" : adj > 0 ? "text-emerald-400" : ""}`}>
                        {adj !== 0 ? `${adj > 0 ? "+" : ""}¥${adj.toLocaleString()}` : "—"}
                      </TableCell>
                      <TableCell className="text-right text-sm tabular-nums font-medium">¥{Number(item.amount ?? 0).toLocaleString()}</TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>

          {/* 入金記録 */}
          {(data.payments ?? []).length > 0 && (
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <div className="px-4 py-2 border-b border-border">
                <span className="text-sm font-medium text-foreground">💰 入金記録</span>
              </div>
              <Table>
                <TableHeader>
                  <TableRow className="border-border hover:bg-transparent">
                    <TableHead className="text-xs text-muted-foreground">入金日</TableHead>
                    <TableHead className="text-xs text-muted-foreground text-right">入金額</TableHead>
                    <TableHead className="text-xs text-muted-foreground">方法</TableHead>
                    <TableHead className="text-xs text-muted-foreground">名義/備考</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {(data.payments ?? []).map((p: Record<string, unknown>, i: number) => (
                    <TableRow key={i} className="border-border/50">
                      <TableCell className="text-sm">{String(p.payment_date)}</TableCell>
                      <TableCell className="text-right text-sm tabular-nums font-medium">¥{Number(p.amount).toLocaleString()}</TableCell>
                      <TableCell className="text-sm">{String(p.method || "—")}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{String(p.reference || "—")}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}

          {/* 備考 */}
          {data.notes && (
            <div className="bg-card border border-border rounded-lg p-4">
              <dt className="text-[11px] text-muted-foreground font-medium mb-1">備考</dt>
              <dd className="text-sm text-foreground whitespace-pre-wrap">{data.notes}</dd>
            </div>
          )}

          {/* PDFプレビューパネル（支払通知詳細と同じUX） */}
          {pdfOpen && pdfUrl && (
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <div className="px-4 py-2 border-b border-border bg-muted/30 flex items-center gap-2 flex-wrap">
                <span className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border bg-blue-500/10 text-blue-400 border-blue-500/30">
                  <FileText className="w-3.5 h-3.5" /> 請求書
                </span>
                <button
                  type="button"
                  onClick={handlePdfDownload}
                  className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-border text-muted-foreground hover:text-foreground transition-colors"
                >
                  <Download className="w-3.5 h-3.5" /> ダウンロード
                </button>
                <div className="flex-1" />
                <button
                  type="button"
                  onClick={() => setPdfOpen(false)}
                  className="p-1.5 text-muted-foreground hover:text-foreground"
                  aria-label="プレビューを閉じる"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
              <iframe
                key={pdfUrl}
                src={pdfUrl}
                className="w-full h-[640px] bg-white"
                title="請求書PDF"
              />
            </div>
          )}

          {/* 編集モーダル */}
          <FormModal
            open={editOpen}
            title={`請求書 #${id} 編集`}
            size="lg"
            loading={editMutation.isPending}
            onSubmit={() => editMutation.mutate()}
            onClose={() => setEditOpen(false)}
          >
            <div className="grid grid-cols-2 gap-4">
              <FormField label="発行日">
                <FormInput
                  type="date"
                  value={editForm.issue_date}
                  onChange={(e) => setEditForm((f) => ({ ...f, issue_date: e.target.value }))}
                />
              </FormField>
              <FormField label="支払期日">
                <FormInput
                  type="date"
                  value={editForm.due_date}
                  onChange={(e) => setEditForm((f) => ({ ...f, due_date: e.target.value }))}
                />
              </FormField>
            </div>
            <FormField label="件名">
              <FormInput
                value={editForm.subject}
                onChange={(e) => setEditForm((f) => ({ ...f, subject: e.target.value }))}
              />
            </FormField>
            <FormField label="備考">
              <FormTextarea
                rows={5}
                value={editForm.notes}
                onChange={(e) => setEditForm((f) => ({ ...f, notes: e.target.value }))}
              />
            </FormField>
            {editMutation.isError && (
              <p className="text-sm text-red-400">エラー: {(editMutation.error as Error).message}</p>
            )}
          </FormModal>

          {/* 送信メール確認・編集モーダル（背面でPDFプレビューを開いたまま） */}
          <FormModal
            open={sendOpen}
            title={status === "SENT" ? "クライアントへ請求書を再送信" : "クライアントへ請求書を送信"}
            size="lg"
            loading={sendMut.isPending}
            submitLabel={status === "SENT" ? "🔁 再送信する" : "📧 送信する"}
            onSubmit={() => {
              if (!sendMeta.to_email) {
                toast.error("送信先メールアドレスが設定されていません");
                return;
              }
              sendMut.mutate();
            }}
            onClose={() => setSendOpen(false)}
          >
            <p className="text-xs text-muted-foreground mb-3">
              {sendMeta.client_name} 宛に、請求書（発行元: 当社 → 宛先: クライアント、印影付き）の確認・ダウンロード用URLをメール送信します（PDF添付なし）。
              {status === "SENT" && "再送信すると、ダウンロードURLの有効期限も送信日時を起点に更新されます。"}
              詳細下部のPDFプレビューで内容を確認しながら送信できます。
            </p>
            <FormField label="宛先（クライアント請求先）">
              <FormInput
                value={sendMeta.to_email || ""}
                readOnly
                placeholder="未設定 — 受注書またはクライアントマスタの請求書送付先を設定してください"
              />
            </FormField>
            <FormField label="CC（カンマ区切り）">
              <FormInput
                value={sendMeta.cc_email || ""}
                readOnly
                placeholder="未設定"
              />
            </FormField>
            <FormField label="件名">
              <FormInput value={emailForm.subject} onChange={(e) => setEmailForm((f) => ({ ...f, subject: e.target.value }))} />
            </FormField>
            <FormField label="本文">
              <FormTextarea rows={10} value={emailForm.body} onChange={(e) => setEmailForm((f) => ({ ...f, body: e.target.value }))} />
            </FormField>
            {sendMut.isError && (
              <p className="text-sm text-red-400">エラー: {(sendMut.error as Error).message}</p>
            )}
          </FormModal>
        </>
      )}
    </DetailLayout>
  );
}
