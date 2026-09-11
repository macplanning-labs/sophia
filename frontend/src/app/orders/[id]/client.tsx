"use client";

import { useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, useEffect } from "react";
import { fetchOrderDetail, fetchOrderEmailPreview, fetchOrderTimesheetRequestPreview, apiPost, apiPut, apiDelete, apiUpload, apiDownload, downloadBlob } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { StatusBadge } from "@/components/ui/status-badge";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Pencil, Trash2, Copy, FileDown, Send, ChevronDown, Mail } from "lucide-react";
import { toast } from "sonner";
import { GuidanceCallout } from "@/components/ui/guidance-callout";
import { getPurchaseOrderStatusGuidance } from "@/lib/guidance/orders";

const ORDER_STATUSES = [
  { value: "DRAFT", label: "下書き" },
  { value: "SENT", label: "送付済" },
  { value: "ACCEPTED", label: "受諾済" },
  { value: "REPORT_RECEIVED", label: "報告受領" },
  { value: "NOTICE_CREATED", label: "通知作成" },
  { value: "NOTICE_CONFIRMED", label: "通知受諾" },
  { value: "PAID", label: "支払済" },
];

const SETTLEMENT_TYPES = [
  { value: "上下割", label: "上下割" },
  { value: "中間割", label: "中間割" },
  { value: "固定時間", label: "固定時間" },
  { value: "一律割", label: "一律割" },
  { value: "RANGE", label: "RANGE" },
];

interface OrderItem {
  id: number;
  order_id: string;
  partner_contract_id: number;
  base_fee: number;
  effort: string;
  actual_hours: string;
  settlement_type: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  fixed_hours?: string;
  deduction_rate: number;
  overtime_rate: number;
  adjustment: number;
  price: number;
  engineer_name: string;
}

export default function OrderDetailPage({
  orderId,
  embedded = false,
  onDeleted,
}: {
  /** 指定時はURLではなくこのIDで詳細を取得(一覧の行展開埋め込み用) */
  orderId?: string;
  embedded?: boolean;
  onDeleted?: () => void;
} = {}) {
  const routeId = useDynamicId();
  const id = orderId || routeId;
  const router = useRouter();
  const qc = useQueryClient();

  const invalidateOrderViews = () => {
    qc.invalidateQueries({ queryKey: ["orders"] });
    qc.invalidateQueries({ queryKey: ["orders", id] });
    qc.invalidateQueries({ queryKey: ["dashboard"] });
    qc.invalidateQueries({ queryKey: ["notifications"] });
  };

  const { data, isLoading } = useQuery({
    queryKey: ["orders", id],
    queryFn: () => fetchOrderDetail(id),
    enabled: !!id,
  });

  const order = data?.order;
  const items: OrderItem[] = data?.items ?? [];
  const isDraft = order?.status === "DRAFT";

  // ── ステータス変更 ──
  const [statusMenuOpen, setStatusMenuOpen] = useState(false);
  const statusMutation = useMutation({
    mutationFn: (status: string) => apiPost(`/api/v1/orders/${id}/status`, { status }),
    onSuccess: () => {
      invalidateOrderViews();
      setStatusMenuOpen(false);
    },
    onError: (e: Error) => { toast.error(`ステータス変更に失敗しました: ${e.message}`); },
  });

  // ── 送付 ──
  const [publishOpen, setPublishOpen] = useState(false);
  const [publishForm, setPublishForm] = useState({ recipient: "", cc: "", subject: "", body: "" });
  const publishMutation = useMutation({
    mutationFn: () => apiPost<{ success: boolean; message?: string }>(`/api/v1/orders/${id}/publish`, publishForm),
    onSuccess: (res) => {
      invalidateOrderViews();
      toast.success(res.message || "送付しました");
      setPublishOpen(false);
    },
    onError: (e: Error) => { toast.error(`送付に失敗しました: ${e.message}`); setPublishOpen(false); },
  });
  const openPublishModal = async () => {
    try {
      const preview = await fetchOrderEmailPreview(id);
      setPublishForm({
        recipient: data?.partner_email || "",
        cc: "",
        subject: preview.subject,
        body: preview.body,
      });
      setPublishOpen(true);
    } catch (e) {
      toast.error(`メールプレビューの取得に失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };

  // ── 訂正再送付（SENT/ACCEPTED時） ──
  const republishMutation = useMutation({
    mutationFn: () => apiPost<{ success: boolean; message?: string }>(`/api/v1/orders/${id}/republish`, {}),
    onSuccess: (res) => {
      invalidateOrderViews();
      toast.success(res.message || "訂正再送付しました");
    },
    onError: (e: Error) => { toast.error(`訂正再送付に失敗しました: ${e.message}`); },
  });

  // ── 稼働報告提出依頼メール ──
  const [requestTimesheetOpen, setRequestTimesheetOpen] = useState(false);
  const [requestTimesheetForm, setRequestTimesheetForm] = useState({ subject: "", body: "" });
  const [requestTimesheetTo, setRequestTimesheetTo] = useState("");
  const [requestTimesheetFile, setRequestTimesheetFile] = useState<File | null>(null);
  const requestTimesheetMutation = useMutation({
    mutationFn: () => {
      const formData = new FormData();
      formData.append("subject", requestTimesheetForm.subject);
      formData.append("body", requestTimesheetForm.body);
      if (requestTimesheetFile) formData.append("file", requestTimesheetFile);
      return apiUpload<{ success: boolean; message?: string }>(`/api/v1/orders/${id}/request-timesheet`, formData);
    },
    onSuccess: (res) => {
      setRequestTimesheetOpen(false);
      setRequestTimesheetFile(null);
      toast.success(res.message || "提出依頼メールを送信しました");
    },
    onError: (e: Error) => {
      toast.error(e.message || "提出依頼メールの送信に失敗しました");
    },
  });
  const openRequestTimesheetModal = async () => {
    try {
      const preview = await fetchOrderTimesheetRequestPreview(id);
      setRequestTimesheetForm({ subject: preview.subject, body: preview.body });
      setRequestTimesheetTo(preview.to_email || "");
      setRequestTimesheetOpen(true);
    } catch (e) {
      toast.error(`メールプレビューの取得に失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };

  // ── 削除 ──
  const [deleteOpen, setDeleteOpen] = useState(false);
  const deleteMutation = useMutation({
    mutationFn: () => apiDelete(`/api/v1/orders/${id}`),
    onSuccess: () => {
      invalidateOrderViews();
      if (embedded) {
        onDeleted?.();
      } else {
        router.push("/");
      }
    },
    onError: (e: Error) => { toast.error(`削除に失敗しました: ${e.message}`); setDeleteOpen(false); },
  });

  // ── ロールフォワード ──
  const [rollforwardOpen, setRollforwardOpen] = useState(false);
  const rollforwardMutation = useMutation({
    mutationFn: () => apiPost<{ success: boolean; order_id?: string }>(`/api/v1/orders/${id}/rollforward`, {}),
    onSuccess: (res) => {
      invalidateOrderViews();
      if (res.order_id) {
        if (embedded) {
          toast.success(`翌月コピーを作成しました: ${res.order_id}`);
        } else {
          router.push(`/orders/${res.order_id}`);
        }
      }
    },
    onError: (e: Error) => { toast.error(`翌月コピーに失敗しました: ${e.message}`); },
  });

  // ── PDFプレビュー（Blob取得 → 新タブで表示） ──
  const handlePdfPreview = async () => {
    try {
      const blob = await apiDownload(`/api/v1/orders/${id}/pdf`);
      const url = URL.createObjectURL(blob);
      window.open(url, '_blank');
    } catch (e) {
      toast.error(`PDF取得に失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };
  const handleAcceptancePdfPreview = async () => {
    try {
      const blob = await apiDownload(`/api/v1/orders/${id}/acceptance-pdf`);
      const url = URL.createObjectURL(blob);
      window.open(url, '_blank');
    } catch (e) {
      toast.error(`PDF取得に失敗しました: ${e instanceof Error ? e.message : e}`);
    }
  };
  // ── PDFダウンロード ──
  const handlePdfDownload = async () => {
    try {
      const blob = await apiDownload(`/api/v1/orders/${id}/pdf`);
      downloadBlob(blob, `${id}.pdf`);
    } catch (e) {
      console.error("PDF download failed:", e);
    }
  };
  const handleAcceptancePdfDownload = async () => {
    try {
      const blob = await apiDownload(`/api/v1/orders/${id}/acceptance-pdf`);
      downloadBlob(blob, `${id}_acceptance.pdf`);
    } catch (e) {
      console.error("Acceptance PDF download failed:", e);
    }
  };

  // ── 編集モーダル ──
  const [editOpen, setEditOpen] = useState(false);
  const [headerForm, setHeaderForm] = useState({
    work_start: "",
    work_end: "",
    work_location: "",
    payment_condition: "",
  });
  const [itemForms, setItemForms] = useState<{
    id: number;
    engineer_name: string;
    base_fee: string;
    effort: string;
    settlement_type: string;
    lower_limit_hours: string;
    upper_limit_hours: string;
    deduction_rate: string;
    overtime_rate: string;
  }[]>([]);

  useEffect(() => {
    if (order && editOpen) {
      setHeaderForm({
        work_start: order.work_start || "",
        work_end: order.work_end || "",
        work_location: order.work_location || "",
        payment_condition: order.payment_condition || "",
      });
      setItemForms(items.map((item) => ({
        id: item.id,
        engineer_name: item.engineer_name,
        base_fee: String(item.base_fee),
        effort: String(item.effort),
        settlement_type: item.settlement_type,
        lower_limit_hours: String(item.lower_limit_hours),
        upper_limit_hours: String(item.upper_limit_hours),
        deduction_rate: String(item.deduction_rate),
        overtime_rate: String(item.overtime_rate),
      })));
    }
  }, [order, editOpen]);

  const updateMutation = useMutation({
    mutationFn: () => apiPut(`/api/v1/orders/${id}`, {
      ...headerForm,
      items: itemForms.map((f) => ({
        id: f.id,
        base_fee: Number(f.base_fee) || 0,
        effort: f.effort,
        settlement_type: f.settlement_type,
        lower_limit_hours: f.lower_limit_hours,
        upper_limit_hours: f.upper_limit_hours,
        deduction_rate: Number(f.deduction_rate) || 0,
        overtime_rate: Number(f.overtime_rate) || 0,
      })),
    }),
    onSuccess: () => {
      invalidateOrderViews();
      setEditOpen(false);
    },
  });

  const setItemField = (idx: number, key: string, value: string) => {
    setItemForms((prev) => prev.map((f, i) =>
      i === idx ? { ...f, [key]: value } : f
    ));
  };

  // 金額計算
  const subtotal = items.reduce((sum, item) => sum + Number(item.price), 0);
  const taxAmount = Math.floor(subtotal * 0.1);
  const total = subtotal + taxAmount;

  return (
    <DetailLayout title={`発注書 ${id}`} icon="🧾" backHref="/" backLabel="ダッシュボードに戻る" isLoading={isLoading} embedded={embedded}>
      {order && (
        <>
          {(() => {
            const g = getPurchaseOrderStatusGuidance(order.status, id);
            return g ? (
              <div className="mb-4">
                <GuidanceCallout variant="info" {...g} />
              </div>
            ) : null;
          })()}
          {/* アクションバー — SSR版と同じ順序: 送付, PDF, 注文請書PDF, ステータス変更, 翌月コピー, 編集, 削除 */}
          <div className="flex flex-wrap items-center gap-2 mb-4 p-3 bg-card border border-border rounded-lg">
            {/* 送付ボタン（DRAFT時のみ） */}
            {isDraft && (
              <Button
                variant="outline"
                size="sm"
                className="border-emerald-700/50 text-emerald-400 hover:text-emerald-300 hover:border-emerald-600 gap-1"
                onClick={openPublishModal}
              >
                <Send className="w-3.5 h-3.5" /> 送付
              </Button>
            )}

            {/* 訂正再送付ボタン（SENT/ACCEPTED時のみ） */}
            {(order.status === "SENT" || order.status === "ACCEPTED") && (
              <Button
                variant="outline"
                size="sm"
                className="border-amber-700/50 text-amber-400 hover:text-amber-300 hover:border-amber-600 gap-1"
                onClick={() => { if (confirm("内容を訂正して再送付しますか？パートナーに再度メールが送信されます。")) republishMutation.mutate(); }}
                disabled={republishMutation.isPending}
              >
                <Send className="w-3.5 h-3.5" /> 訂正再送付
              </Button>
            )}

            {/* 稼働報告提出依頼（SENT/ACCEPTED時） */}
            {(order.status === "SENT" || order.status === "ACCEPTED") && (
              <Button
                variant="outline"
                size="sm"
                className="border-sky-700/50 text-sky-400 hover:text-sky-300 hover:border-sky-600 gap-1"
                onClick={openRequestTimesheetModal}
                disabled={requestTimesheetMutation.isPending}
              >
                <Mail className="w-3.5 h-3.5" /> 提出依頼メール
              </Button>
            )}

            <Button variant="outline" size="sm" className="border-border text-foreground gap-1" onClick={handlePdfPreview}>
              <FileDown className="w-3.5 h-3.5" /> 注文書PDF
            </Button>

            <Button variant="outline" size="sm" className="border-border text-foreground gap-1" onClick={handleAcceptancePdfPreview}>
              <FileDown className="w-3.5 h-3.5" /> 注文請書PDF
            </Button>

            {/* ステータス変更 */}
            <div className="relative">
              <Button
                variant="outline"
                size="sm"
                className="border-border text-foreground hover:text-foreground gap-1"
                onClick={() => setStatusMenuOpen(!statusMenuOpen)}
              >
                ステータス変更 <ChevronDown className="w-3 h-3" />
              </Button>
              {statusMenuOpen && (
                <div className="absolute top-full left-0 mt-1 z-20 bg-card border border-border rounded-lg shadow-xl py-1 min-w-[160px]">
                  {ORDER_STATUSES.map((s) => (
                    <button
                      key={s.value}
                      onClick={() => statusMutation.mutate(s.value)}
                      disabled={s.value === order.status || statusMutation.isPending}
                      className="w-full text-left px-3 py-1.5 text-sm text-foreground hover:bg-muted disabled:opacity-40 disabled:cursor-default"
                    >
                      {s.label} {s.value === order.status && "✓"}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <Button
              variant="outline"
              size="sm"
              className="border-border text-foreground gap-1"
              onClick={() => setRollforwardOpen(true)}
            >
              <Copy className="w-3.5 h-3.5" /> 翌月コピー
            </Button>

            {isDraft && (
              <Button
                variant="outline"
                size="sm"
                className="border-border text-foreground gap-1"
                onClick={() => setEditOpen(true)}
              >
                <Pencil className="w-3.5 h-3.5" /> 編集
              </Button>
            )}

            {isDraft && (
              <Button
                variant="outline"
                size="sm"
                className="border-red-700/50 text-red-400 hover:text-red-300 gap-1 ml-auto"
                onClick={() => setDeleteOpen(true)}
              >
                <Trash2 className="w-3.5 h-3.5" /> 削除
              </Button>
            )}
          </div>

          {/* 注文書情報 + 金額サマリ — SSR版の2カラムレイアウト */}
          <div className="grid grid-cols-1 lg:grid-cols-5 gap-4 mb-4">
            {/* 注文書情報（左） */}
            <div className="lg:col-span-3 bg-card border border-border rounded-lg p-4">
              <h3 className="text-sm font-medium text-foreground mb-3">📋 注文書情報</h3>
              <FieldGrid>
                <Field label="注文番号" value={<code className="text-sm">{id}</code>} />
                <Field label="パートナー" value={data.partner_name} />
                <Field label="案件" value={data.project_name} />
                <Field label="ステータス" value={<StatusBadge status={order.status} />} />
              </FieldGrid>
              <FieldGrid>
                <Field label="発注日" value={order.order_date} />
                <Field label="作業期間" value={`${order.work_start} 〜 ${order.work_end}`} />
                <Field label="作業場所" value={order.work_location || "—"} />
                <Field label="支払条件" value={order.payment_condition || "—"} />
              </FieldGrid>
            </div>

            {/* 金額サマリ（右） */}
            <div className="lg:col-span-2 bg-card border border-border rounded-lg p-4">
              <h3 className="text-sm font-medium text-foreground mb-3">💰 金額</h3>
              <div className="grid grid-cols-3 gap-4 text-center">
                <div>
                  <div className="text-xs text-muted-foreground mb-1">小計（税抜）</div>
                  <div className="text-lg font-bold text-foreground">¥{subtotal.toLocaleString()}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground mb-1">消費税</div>
                  <div className="text-lg text-foreground">¥{taxAmount.toLocaleString()}</div>
                </div>
                <div>
                  <div className="text-xs text-muted-foreground mb-1">合計（税込）</div>
                  <div className="text-xl font-bold text-blue-400">¥{total.toLocaleString()}</div>
                </div>
              </div>
            </div>
          </div>

          {/* 明細テーブル — SSR版に合わせて精算幅・調整金カラムを追加 */}
          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-border">
              <span className="text-sm font-medium text-foreground">発注明細 ({items.length}件)</span>
            </div>
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">要員</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">単価</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-center">工数</TableHead>
                  <TableHead className="text-xs text-muted-foreground">精算幅</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">実稼働</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">調整金</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">金額</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item, i) => (
                  <TableRow key={i} className="border-border/50">
                    <TableCell className="text-sm font-medium">{String(item.engineer_name)}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">¥{Number(item.base_fee).toLocaleString()}</TableCell>
                    <TableCell className="text-center text-sm tabular-nums">{String(item.effort)}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {item.lower_limit_hours && item.upper_limit_hours
                        ? `${item.lower_limit_hours}〜${item.upper_limit_hours}h`
                        : "—"}
                    </TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{String(item.actual_hours)} h</TableCell>
                    <TableCell className={`text-right text-sm tabular-nums ${Number(item.adjustment ?? 0) < 0 ? "text-red-400" : Number(item.adjustment ?? 0) > 0 ? "text-emerald-400" : "text-muted-foreground"}`}>
                      {Number(item.adjustment ?? 0) !== 0
                        ? `${Number(item.adjustment ?? 0) > 0 ? "+" : ""}¥${Number(item.adjustment ?? 0).toLocaleString()}`
                        : "—"}
                    </TableCell>
                    <TableCell className="text-right text-sm tabular-nums font-medium">¥{Number(item.price).toLocaleString()}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          {/* 編集モーダル */}
          <FormModal
            open={editOpen}
            title={`発注書 ${id} 編集`}
            size="xl"
            loading={updateMutation.isPending}
            onSubmit={() => updateMutation.mutate()}
            onClose={() => setEditOpen(false)}
          >
            <div className="grid grid-cols-3 gap-4">
              <FormField label="稼働開始">
                <FormInput type="date" value={headerForm.work_start} onChange={(e) => setHeaderForm((p) => ({ ...p, work_start: e.target.value }))} />
              </FormField>
              <FormField label="稼働終了">
                <FormInput type="date" value={headerForm.work_end} onChange={(e) => setHeaderForm((p) => ({ ...p, work_end: e.target.value }))} />
              </FormField>
              <FormField label="作業場所">
                <FormInput value={headerForm.work_location} onChange={(e) => setHeaderForm((p) => ({ ...p, work_location: e.target.value }))} />
              </FormField>
            </div>
            <div className="grid grid-cols-1 gap-4 mt-3">
              <FormField label="支払条件">
                <FormInput value={headerForm.payment_condition} onChange={(e) => setHeaderForm((p) => ({ ...p, payment_condition: e.target.value }))} />
              </FormField>
            </div>

            {itemForms.length > 0 && (
              <div className="mt-4 space-y-3">
                <span className="text-xs font-medium text-muted-foreground">明細</span>
                {itemForms.map((f, idx) => (
                  <div key={f.id} className="border border-border rounded-lg p-3">
                    <div className="text-xs text-muted-foreground mb-2 font-medium">{f.engineer_name || `明細 #${f.id}`}</div>
                    <div className="grid grid-cols-4 gap-3">
                      <FormField label="単金">
                        <FormInput type="number" value={f.base_fee} onChange={(e) => setItemField(idx, "base_fee", e.target.value)} />
                      </FormField>
                      <FormField label="工数">
                        <FormInput type="number" step="0.1" value={f.effort} onChange={(e) => setItemField(idx, "effort", e.target.value)} />
                      </FormField>
                      <FormField label="精算方式">
                        <FormSelect options={SETTLEMENT_TYPES} value={f.settlement_type} onChange={(e) => setItemField(idx, "settlement_type", e.target.value)} />
                      </FormField>
                      <FormField label="下限時間">
                        <FormInput type="number" value={f.lower_limit_hours} onChange={(e) => setItemField(idx, "lower_limit_hours", e.target.value)} />
                      </FormField>
                    </div>
                    <div className="grid grid-cols-3 gap-3 mt-2">
                      <FormField label="上限時間">
                        <FormInput type="number" value={f.upper_limit_hours} onChange={(e) => setItemField(idx, "upper_limit_hours", e.target.value)} />
                      </FormField>
                      <FormField label="控除単価">
                        <FormInput type="number" value={f.deduction_rate} onChange={(e) => setItemField(idx, "deduction_rate", e.target.value)} />
                      </FormField>
                      <FormField label="超過単価">
                        <FormInput type="number" value={f.overtime_rate} onChange={(e) => setItemField(idx, "overtime_rate", e.target.value)} />
                      </FormField>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {updateMutation.isError && (
              <p className="text-sm text-red-400">エラー: {(updateMutation.error as Error).message}</p>
            )}
          </FormModal>

          {/* 担当者情報 */}
          {(order?.甲_責任者 || order?.乙_責任者) && (
            <div className="bg-card border border-border rounded-lg p-4 mt-4">
              <h3 className="text-sm font-medium text-foreground mb-3">👥 担当者情報</h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <h4 className="text-xs font-medium text-muted-foreground mb-2">甲（当社）</h4>
                  <FieldGrid>
                    <Field label="業務責任者" value={order?.甲_責任者 || "—"} />
                    <Field label="連絡窓口" value={order?.甲_担当者 || "—"} />
                    <Field label="作業責任者" value={order?.作業責任者 || "—"} />
                  </FieldGrid>
                </div>
                <div>
                  <h4 className="text-xs font-medium text-muted-foreground mb-2">乙（パートナー）</h4>
                  <FieldGrid>
                    <Field label="業務責任者" value={order?.乙_責任者 || "—"} />
                    <Field label="連絡窓口" value={order?.乙_担当者 || "—"} />
                  </FieldGrid>
                </div>
              </div>
            </div>
          )}

          {/* 送付モーダル（メール編集） */}
          <FormModal
            open={publishOpen}
            title={`発注書 ${id} — メール送付`}
            size="xl"
            loading={publishMutation.isPending}
            onSubmit={() => publishMutation.mutate()}
            onClose={() => setPublishOpen(false)}
            submitLabel="送付する"
          >
            <div className="space-y-3">
              <FormField label="宛先">
                <FormInput type="email" value={publishForm.recipient} onChange={(e) => setPublishForm((p) => ({ ...p, recipient: e.target.value }))} />
              </FormField>
              <FormField label="CC（カンマ区切り）">
                <FormInput value={publishForm.cc} onChange={(e) => setPublishForm((p) => ({ ...p, cc: e.target.value }))} />
              </FormField>
              <FormField label="件名">
                <FormInput value={publishForm.subject} onChange={(e) => setPublishForm((p) => ({ ...p, subject: e.target.value }))} />
              </FormField>
              <FormField label="本文">
                <textarea
                  className="w-full bg-muted border border-border rounded-md px-3 py-2 text-sm text-foreground min-h-[200px] focus:outline-none focus:ring-2 focus:ring-blue-500"
                  value={publishForm.body}
                  onChange={(e) => setPublishForm((p) => ({ ...p, body: e.target.value }))}
                  rows={12}
                />
              </FormField>
            </div>
          </FormModal>

          {/* 確認ダイアログ */}
          <ConfirmDialog
            open={deleteOpen}
            title="発注書を削除"
            description={`発注書 ${id} を削除してよろしいですか？この操作は取り消せません。`}
            confirmLabel="削除する"
            variant="danger"
            loading={deleteMutation.isPending}
            onConfirm={() => deleteMutation.mutate()}
            onCancel={() => setDeleteOpen(false)}
          />
          <ConfirmDialog
            open={rollforwardOpen}
            title="翌月ロールフォワード"
            description={`${id} を基に翌月の発注書を作成しますか？`}
            confirmLabel="作成する"
            loading={rollforwardMutation.isPending}
            onConfirm={() => rollforwardMutation.mutate()}
            onCancel={() => setRollforwardOpen(false)}
          />
          <FormModal
            open={requestTimesheetOpen}
            title="稼働報告の提出依頼メール"
            size="lg"
            loading={requestTimesheetMutation.isPending}
            submitLabel="送信する"
            onSubmit={() => requestTimesheetMutation.mutate()}
            onClose={() => setRequestTimesheetOpen(false)}
          >
            <div className="space-y-3">
              <FormField label="宛先">
                <FormInput value={requestTimesheetTo} readOnly placeholder="メール未設定" />
              </FormField>
              <FormField label="件名">
                <FormInput
                  value={requestTimesheetForm.subject}
                  onChange={(e) => setRequestTimesheetForm((f) => ({ ...f, subject: e.target.value }))}
                />
              </FormField>
              <FormField label="本文">
                <textarea
                  className="w-full bg-muted border border-border rounded-md px-3 py-2 text-sm text-foreground min-h-[200px] focus:outline-none focus:ring-2 focus:ring-blue-500"
                  value={requestTimesheetForm.body}
                  onChange={(e) => setRequestTimesheetForm((f) => ({ ...f, body: e.target.value }))}
                  rows={12}
                />
              </FormField>
              <FormField label="勤務表PDF添付（任意）">
                <input
                  type="file"
                  accept=".pdf,application/pdf"
                  onChange={(e) => setRequestTimesheetFile(e.target.files?.[0] ?? null)}
                  className="w-full text-sm text-foreground file:mr-3 file:px-3 file:py-1.5 file:rounded-md file:border file:border-border file:bg-muted file:text-foreground file:text-xs"
                />
                <p className="text-xs text-muted-foreground mt-1">
                  クライアント経由で受け取った勤務表PDFを添付できます（任意）。
                </p>
              </FormField>
            </div>
          </FormModal>
        </>
      )}
    </DetailLayout>
  );
}
