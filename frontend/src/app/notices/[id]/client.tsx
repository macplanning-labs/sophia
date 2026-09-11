"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { fetchNoticeDetail, fetchNoticeEmailPreview, sendNoticeMail, sendNoticePeppol, downloadBlob, apiPut } from "@/lib/api";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { StatusBadge } from "@/components/ui/status-badge";
import { FormModal, FormField, FormInput, FormTextarea } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FileText, Receipt, Download, Send, X, Globe } from "lucide-react";
import { useDynamicId, cn } from "@/lib/utils";
import { toast } from "sonner";

export default function NoticeDetailPage({
  noticeId,
  embedded = false,
}: {
  noticeId?: string;
  embedded?: boolean;
} = {}) {
  const routeId = useDynamicId();
  const id = noticeId || routeId;
  const qc = useQueryClient();

  const { data, isLoading } = useQuery({
    queryKey: ["notices", id],
    queryFn: () => fetchNoticeDetail(id),
    enabled: !!id,
  });

  const notice = data?.notice;
  const items = data?.items ?? [];

  // ── 編集モーダル ──
  const [editOpen, setEditOpen] = useState(false);
  const [editForm, setEditForm] = useState({
    notice_date: "",
    payment_due_date: "",
    remarks: "",
  });

  const editMutation = useMutation({
    mutationFn: () => apiPut(`/api/v1/notices/${id}`, editForm),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["notices", id] });
      qc.invalidateQueries({ queryKey: ["notices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setEditOpen(false);
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(`更新に失敗しました: ${e.message}`),
  });

  const openEditModal = () => {
    if (notice) {
      setEditForm({
        notice_date: notice.notice_date || "",
        payment_due_date: notice.payment_due_date || "",
        remarks: data?.remarks || "",
      });
      setEditOpen(true);
    }
  };

  // ── PDFプレビュー（支払通知書 / 代理請求書） ──
  const [pdfOpen, setPdfOpen] = useState(false);
  const [pdfType, setPdfType] = useState<"payment-notice" | "invoice">("payment-notice");

  const pdfUrl = pdfOpen
    ? pdfType === "payment-notice"
      ? `/api/v1/notices/${id}/pdf`
      : `/api/v1/notices/${id}/invoice-pdf`
    : null;

  const handlePdfDownload = async () => {
    if (!pdfUrl) return;
    try {
      const res = await fetch(pdfUrl, { credentials: "include" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const blob = await res.blob();
      const filename = pdfType === "payment-notice" ? `notice_${id}.pdf` : `invoice_${id}.pdf`;
      downloadBlob(blob, filename);
    } catch (e) {
      toast.error(`PDFダウンロードに失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };

  // ── メール送信 ──
  const [sendOpen, setSendOpen] = useState(false);
  const [emailForm, setEmailForm] = useState({ subject: "", body: "" });
  const [sendMeta, setSendMeta] = useState<{ to_email?: string | null; cc_email?: string | null; partner_name?: string }>({});

  const openSendModal = async () => {
    try {
      const preview = await fetchNoticeEmailPreview(id);
      setEmailForm({ subject: preview.subject, body: preview.body });
      setSendMeta({
        to_email: preview.to_email,
        cc_email: preview.cc_email,
        partner_name: preview.partner_name,
      });
      setSendOpen(true);
    } catch (e) {
      toast.error(`メール本文の取得に失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };

  const sendMutation = useMutation({
    mutationFn: () => sendNoticeMail(id, emailForm),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["notices", id] });
      qc.invalidateQueries({ queryKey: ["notices"] });
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      setSendOpen(false);
      toast.success(res.message || "確認URLをメール送信しました");
    },
    onError: (e: Error) => {
      toast.error(e.message || "メール送信に失敗しました");
    },
  });

  const peppolSendMut = useMutation({
    mutationFn: () => sendNoticePeppol(id),
    onSuccess: (res) => {
      if (res.success === false) { toast.error(`Peppol送信に失敗しました: ${res.error}`); return; }
      toast.success(`Peppol経由で送信しました（メッセージID: ${res.message_id}）`);
    },
    onError: (e: Error) => toast.error(`Peppol送信に失敗しました: ${e.message}`),
  });

  return (
    <DetailLayout title={`支払通知書 ${id}`} icon="💳" backHref="/" backLabel="ダッシュボードに戻る" isLoading={isLoading} embedded={embedded}>
      {notice && (
        <>
          {/* アクションバー */}
          <div className="flex items-center gap-2 mb-4 p-3 bg-card border border-border rounded-lg flex-wrap">
            <Button
              variant="outline"
              size="sm"
              className="border-border text-foreground gap-1"
              onClick={() => {
                setPdfType("payment-notice");
                setPdfOpen(true);
              }}
            >
              <FileText className="w-3.5 h-3.5" /> PDF
            </Button>
            <Button
              variant="outline" size="sm"
              className="border-blue-600/50 text-blue-400 hover:text-blue-300 gap-1"
              onClick={openSendModal}
            >
              <Send className="w-3.5 h-3.5" /> {notice.mail_sent_at ? "再送信" : "メール送信"}
            </Button>
            <Button
              variant="outline" size="sm"
              className="border-indigo-600/50 text-indigo-400 hover:text-indigo-300 gap-1"
              onClick={() => peppolSendMut.mutate()}
              disabled={peppolSendMut.isPending}
            >
              <Globe className="w-3.5 h-3.5" /> Peppol送信
            </Button>
            {!notice.confirmed && (
              <Button
                variant="outline"
                size="sm"
                className="border-indigo-600/50 text-indigo-400 hover:text-indigo-300 gap-1"
                onClick={openEditModal}
              >
                ✏️ 編集
              </Button>
            )}
            {notice.mail_sent_at && (
              <span className="text-xs text-muted-foreground ml-2">
                送信済: {String(notice.mail_sent_at).slice(0, 16).replace("T", " ")}
              </span>
            )}
          </div>

          <FieldGrid>
            <Field label="パートナー" value={data.partner_name} />
            <Field label="対象月" value={data.target_month_display} />
            <Field label="通知日" value={notice.notice_date} />
            <Field label="確認状態" value={
              <StatusBadge status={notice.confirmed_at || notice.confirmed ? "CONFIRMED" : "UNCONFIRMED"} />
            } />
          </FieldGrid>

          {/* 元注文書リンク */}
          {data.purchase_order_id && !embedded && (
            <FieldGrid cols={2}>
              <Field label="元注文書" value={
                <a href={`/orders/${data.purchase_order_id}`} className="text-blue-400 hover:underline">
                  <code>{data.purchase_order_id}</code>
                </a>
              } />
              <Field label="支払期日" value={notice.payment_due_date || "—"} />
            </FieldGrid>
          )}
          {data.purchase_order_id && embedded && (
            <FieldGrid cols={2}>
              <Field label="元注文書" value={<code>{data.purchase_order_id}</code>} />
              <Field label="支払期日" value={notice.payment_due_date || "—"} />
            </FieldGrid>
          )}

          {/* 金額サマリ */}
          <div className="bg-card border border-border rounded-lg p-4">
            <div className="grid grid-cols-3 text-center gap-4">
              <div>
                <div className="text-[11px] text-muted-foreground font-medium">小計（税抜）</div>
                <div className="text-lg font-bold text-foreground">¥{Number(data.subtotal ?? 0).toLocaleString()}</div>
              </div>
              <div>
                <div className="text-[11px] text-muted-foreground font-medium">消費税</div>
                <div className="text-lg text-foreground">¥{Number(data.tax_amount ?? 0).toLocaleString()}</div>
              </div>
              <div>
                <div className="text-[11px] text-muted-foreground font-medium">合計（税込）</div>
                <div className="text-xl font-bold text-blue-400">¥{Number(data.total ?? 0).toLocaleString()}</div>
              </div>
            </div>
          </div>

          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-border">
              <span className="text-sm font-medium text-foreground">支払明細 ({items.length}件)</span>
            </div>
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">作業者</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">単金</TableHead>
                  <TableHead className="text-xs text-muted-foreground">精算幅</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">実績時間</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">調整金</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">金額</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item: Record<string, unknown>, i: number) => (
                  <TableRow key={i} className="border-border/50">
                    <TableCell className="text-sm font-medium">{String(item.engineer_name ?? "—")}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">¥{Number(item.base_fee ?? 0).toLocaleString()}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{item.lower_limit_hours && item.upper_limit_hours ? `${item.lower_limit_hours}〜${item.upper_limit_hours}h` : "—"}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{String(item.actual_hours ?? "—")} h</TableCell>
                    <TableCell className={`text-right text-sm tabular-nums ${Number(item.adjustment ?? 0) < 0 ? 'text-red-400' : Number(item.adjustment ?? 0) > 0 ? 'text-emerald-400' : ''}`}>
                      {Number(item.adjustment ?? 0) !== 0 ? `${Number(item.adjustment) > 0 ? '+' : ''}¥${Number(item.adjustment).toLocaleString()}` : "—"}
                    </TableCell>
                    <TableCell className="text-right text-sm tabular-nums font-medium">¥{Number(item.amount ?? 0).toLocaleString()}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          {/* PDFプレビューパネル */}
          {pdfOpen && pdfUrl && (
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <div className="px-4 py-2 border-b border-border bg-muted/30 flex items-center gap-2 flex-wrap">
                <button
                  type="button"
                  onClick={() => setPdfType("payment-notice")}
                  className={cn(
                    "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
                    pdfType === "payment-notice"
                      ? "bg-blue-500/10 text-blue-400 border-blue-500/30"
                      : "border-border text-muted-foreground hover:text-foreground"
                  )}
                >
                  <Receipt className="w-3.5 h-3.5" /> 支払通知書
                </button>
                <button
                  type="button"
                  onClick={() => setPdfType("invoice")}
                  className={cn(
                    "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
                    pdfType === "invoice"
                      ? "bg-blue-500/10 text-blue-400 border-blue-500/30"
                      : "border-border text-muted-foreground hover:text-foreground"
                  )}
                >
                  <FileText className="w-3.5 h-3.5" /> 請求書（代理作成）
                </button>
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
                title={pdfType === "payment-notice" ? "支払通知書PDF" : "請求書PDF"}
              />
            </div>
          )}

          {/* 編集モーダル */}
          <FormModal
            open={editOpen}
            title={`支払通知書 ${id} 編集`}
            size="lg"
            loading={editMutation.isPending}
            onSubmit={() => editMutation.mutate()}
            onClose={() => setEditOpen(false)}
          >
            <div className="grid grid-cols-2 gap-4">
              <FormField label="通知日">
                <FormInput
                  type="date"
                  value={editForm.notice_date}
                  onChange={(e) => setEditForm((f) => ({ ...f, notice_date: e.target.value }))}
                />
              </FormField>
              <FormField label="支払期日">
                <FormInput
                  type="date"
                  value={editForm.payment_due_date}
                  onChange={(e) => setEditForm((f) => ({ ...f, payment_due_date: e.target.value }))}
                />
              </FormField>
            </div>
            <FormField label="備考">
              <FormTextarea
                rows={5}
                value={editForm.remarks}
                onChange={(e) => setEditForm((f) => ({ ...f, remarks: e.target.value }))}
              />
            </FormField>
            {editMutation.isError && (
              <p className="text-sm text-red-400">エラー: {(editMutation.error as Error).message}</p>
            )}
          </FormModal>

          {/* 送信メール確認・編集モーダル */}
          <FormModal
            open={sendOpen}
            title={notice.mail_sent_at ? "支払通知書・請求書を再送信" : "支払通知書・請求書を送信"}
            size="lg"
            loading={sendMutation.isPending}
            submitLabel={notice.mail_sent_at ? "再送信する" : "送信する"}
            onSubmit={() => {
              if (!sendMeta.to_email) {
                toast.error("送信先メールアドレスが設定されていません");
                return;
              }
              sendMutation.mutate();
            }}
            onClose={() => setSendOpen(false)}
          >
            <p className="text-xs text-muted-foreground mb-3">
              {data.partner_name} 宛に、支払通知書と代理作成請求書の確認・承諾用URLをメール送信します（PDF添付なし）。
              {notice.mail_sent_at && "既に承諾済みの場合でも、URLは引き続き有効です。"}
            </p>
            <FormField label="宛先（パートナーメール）">
              <FormInput
                value={sendMeta.to_email || ""}
                readOnly
                placeholder="未設定 — パートナーマスタのメールアドレスを設定してください"
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
            {sendMutation.isError && (
              <p className="text-sm text-red-400">エラー: {(sendMutation.error as Error).message}</p>
            )}
          </FormModal>
        </>
      )}
    </DetailLayout>
  );
}
