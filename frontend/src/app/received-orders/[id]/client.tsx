"use client";

import { useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchReceivedOrderDetail, linkReceivedOrderContract, apiDelete } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useState, useEffect } from "react";
import { toast } from "sonner";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";

// バックエンド(received_orders.rs::update_status)が受け付ける値・home.rsのRO_STEPSと一致させる
const STATUS_OPTIONS = [
  { value: "REGISTERED", label: "受注" },
  { value: "REPORT_RECEIVED", label: "勤怠（報告受領）" },
  { value: "REPORT_SENT", label: "送付" },
  { value: "INVOICED", label: "請求書" },
  { value: "INVOICE_SENT", label: "請求送付" },
  { value: "INVOICE_CONFIRMED", label: "受諾" },
  { value: "PAID", label: "入金" },
];

// 発注書側(orders/[id]/client.tsx)のSETTLEMENT_TYPESと同一
const SETTLEMENT_TYPES = [
  { value: "上下割", label: "上下割" },
  { value: "中間割", label: "中間割" },
  { value: "固定時間", label: "固定時間" },
  { value: "一律割", label: "一律割" },
  { value: "RANGE", label: "RANGE" },
];

interface ReceivedOrderItem {
  id: number;
  engineer_name: string;
  unit_price: number;
  man_month: string;
  actual_hours: string;
  adjustment: number;
  amount: number;
  settlement_type: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  deduction_rate: number;
  overtime_rate: number;
}

export default function ReceivedOrderDetailPage({
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
  const queryClient = useQueryClient();

  const invalidateViews = () => {
    queryClient.invalidateQueries({ queryKey: ["received-orders"] });
    queryClient.invalidateQueries({ queryKey: ["received-orders", id] });
    queryClient.invalidateQueries({ queryKey: ["dashboard"] });
    queryClient.invalidateQueries({ queryKey: ["notifications"] });
  };

  const [selectedStatus, setSelectedStatus] = useState("");
  const [linkContractId, setLinkContractId] = useState("");
  const [showEdit, setShowEdit] = useState(false);
  const [headerForm, setHeaderForm] = useState({
    project_name: "",
    target_month: "",
    work_start: "",
    work_end: "",
    client_order_number: "",
    payment_condition: "",
    report_to_email: "",
    report_cc_emails: "",
    invoice_to_email: "",
    invoice_cc_emails: "",
    remarks: "",
  });
  const [itemForms, setItemForms] = useState<{
    id: number;
    engineer_name: string;
    unit_price: string;
    man_month: string;
    settlement_type: string;
    lower_limit_hours: string;
    upper_limit_hours: string;
    deduction_rate: string;
    overtime_rate: string;
  }[]>([]);

  const { data, isLoading } = useQuery({
    queryKey: ["received-orders", id],
    queryFn: () => fetchReceivedOrderDetail(id),
    enabled: !!id,
  });

  const order = data?.order;
  const items: ReceivedOrderItem[] = data?.items ?? [];
  const isRegistered = order?.status === "REGISTERED";
  const needsContractLink = !!data?.needs_contract_link;
  const linkCandidates: {
    id: number;
    engineer_name: string;
    project_name: string;
    start_date: string;
    end_date: string;
    is_active: boolean;
    name_matched?: boolean;
  }[] = data?.link_candidates ?? [];

  useEffect(() => {
    if (!needsContractLink) {
      setLinkContractId("");
      return;
    }
    const preferred =
      linkCandidates.find((c) => c.name_matched) ??
      (linkCandidates.length === 1 ? linkCandidates[0] : undefined);
    if (preferred) {
      setLinkContractId(String(preferred.id));
    }
  }, [needsContractLink, linkCandidates]);

  const linkContract = useMutation({
    mutationFn: () => linkReceivedOrderContract(id, Number(linkContractId)),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "紐付けに失敗しました");
        return;
      }
      invalidateViews();
      toast.success(res.message ?? "受注契約を紐付けました");
    },
    onError: (e: Error) => toast.error(`紐付けエラー: ${e.message}`),
  });

  useEffect(() => {
    if (order && showEdit) {
      setHeaderForm({
        project_name: order.project_name ?? "",
        target_month: order.target_month ? String(order.target_month).slice(0, 7) : "",
        work_start: order.work_start ?? "",
        work_end: order.work_end ?? "",
        client_order_number: order.client_order_number ?? "",
        payment_condition: order.payment_condition ?? "",
        report_to_email: order.report_to_email ?? "",
        report_cc_emails: order.report_cc_emails ?? "",
        invoice_to_email: order.invoice_to_email ?? "",
        invoice_cc_emails: order.invoice_cc_emails ?? "",
        remarks: order.remarks ?? "",
      });
      setItemForms(items.map((item) => ({
        id: item.id,
        engineer_name: item.engineer_name,
        unit_price: String(item.unit_price),
        man_month: String(item.man_month),
        settlement_type: item.settlement_type,
        lower_limit_hours: String(item.lower_limit_hours),
        upper_limit_hours: String(item.upper_limit_hours),
        deduction_rate: String(item.deduction_rate),
        overtime_rate: String(item.overtime_rate),
      })));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [order, showEdit]);

  const rollforward = useMutation({
    mutationFn: () => fetch(`/api/v1/received-orders/${id}/rollforward`, { method: "POST" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "ロールフォワードに失敗しました"); } return r; }),
    onSuccess: () => { invalidateViews(); toast.success("ロールフォワード完了"); },
    onError: (e: Error) => toast.error(`ロールフォワードに失敗しました: ${e.message}`),
  });

  const updateStatus = useMutation({
    mutationFn: async (status: string) => { const r = await fetch(`/api/v1/received-orders/${id}/update-status`, {
      method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: `status=${status}`,
    }); if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "ステータス変更に失敗しました"); } return r; },
    onSuccess: () => { invalidateViews(); toast.success("ステータス更新完了"); },
    onError: (e: Error) => toast.error(`ステータス変更に失敗しました: ${e.message}`),
  });

  const sendReport = useMutation({
    mutationFn: () => fetch(`/api/v1/received-orders/${id}/send-report`, { method: "POST" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "送信に失敗しました"); } return r; }),
    onSuccess: () => { invalidateViews(); toast.success("レポート送信完了"); },
    onError: (e: Error) => toast.error(`レポート送信に失敗しました: ${e.message}`),
  });

  const deleteOrder = useMutation({
    mutationFn: () => apiDelete(`/api/v1/received-orders/${id}`),
    onSuccess: (res: { success?: boolean; error?: string }) => {
      if (res?.success === false) {
        toast.error(res.error ?? "削除に失敗しました");
        return;
      }
      invalidateViews();
      if (embedded) {
        onDeleted?.();
      } else {
        router.push("/");
      }
    },
    onError: (e: Error) => toast.error(`削除に失敗しました: ${e.message}`),
  });

  const editMutation = useMutation({
    mutationFn: async () => {
      const r = await fetch(`/api/v1/received-orders/${id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({
          ...headerForm,
          target_month: headerForm.target_month ? `${headerForm.target_month}-01` : headerForm.target_month,
          items: itemForms.map((f) => ({
            id: f.id,
            unit_price: Number(f.unit_price) || 0,
            man_month: f.man_month,
            settlement_type: f.settlement_type,
            lower_limit_hours: f.lower_limit_hours,
            upper_limit_hours: f.upper_limit_hours,
            deduction_rate: Number(f.deduction_rate) || 0,
            overtime_rate: Number(f.overtime_rate) || 0,
          })),
        }),
      });
      if (!r.ok) throw new Error("更新に失敗しました");
      return r.json();
    },
    onSuccess: () => {
      invalidateViews();
      setShowEdit(false);
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const setItemField = (idx: number, key: string, value: string) => {
    setItemForms((prev) => prev.map((f, i) => (i === idx ? { ...f, [key]: value } : f)));
  };

  const actions = (
    <div className="flex items-center gap-2 flex-wrap">
      <button
        onClick={() => setShowEdit(true)}
        className="px-3 py-1.5 text-xs font-medium rounded-md bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 hover:bg-indigo-500/20 transition-colors"
      >
        ✏️ 編集
      </button>
      <button
        onClick={() => { if (confirm("翌月にロールフォワードしますか？")) rollforward.mutate(); }}
        disabled={rollforward.isPending}
        className="px-3 py-1.5 text-xs font-medium rounded-md bg-amber-500/10 text-amber-400 border border-amber-500/30 hover:bg-amber-500/20 transition-colors disabled:opacity-50"
      >
        🔄 ロールフォワード
      </button>
      <button
        onClick={() => sendReport.mutate()}
        disabled={sendReport.isPending}
        className="px-3 py-1.5 text-xs font-medium rounded-md bg-blue-500/10 text-blue-400 border border-blue-500/30 hover:bg-blue-500/20 transition-colors disabled:opacity-50"
      >
        📧 レポート送信
      </button>
      {isRegistered && (
        <button
          onClick={() => { if (confirm("この受注を削除しますか？この操作は取り消せません。")) deleteOrder.mutate(); }}
          disabled={deleteOrder.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-red-500/10 text-red-400 border border-red-500/30 hover:bg-red-500/20 transition-colors disabled:opacity-50"
        >
          🗑 削除
        </button>
      )}
    </div>
  );

  return (
    <DetailLayout title={`受注書 #${id}`} icon="📊" backHref="/" backLabel="ダッシュボードに戻る" isLoading={isLoading} actions={actions} embedded={embedded}>
      {order && (
        <>
          {needsContractLink && (
            <div className="rounded-lg border border-amber-500/40 bg-amber-500/10 px-4 py-3 space-y-3">
              <p className="text-sm text-amber-300 font-medium">
                ⚠ 受注契約が未紐付けです
              </p>
              <p className="text-xs text-amber-200/90">
                EDI・クロス等の注文書には案件IDがなく、取込時に受注契約を一意に特定できないことがあります。
                稼働報告・進捗連携のため、該当する受注契約を紐付けてください。
                件名がマスタ正式名と違う場合は、案件マスタの「EDI案件別名」も設定してください。
              </p>
              {linkCandidates.length === 0 ? (
                <p className="text-xs text-amber-200/80">
                  紐付け候補の受注契約がありません。受注契約画面で期間・技術者を確認してください。
                </p>
              ) : (
                <div className="flex flex-wrap items-end gap-2">
                  <div className="min-w-[16rem] flex-1">
                    <label className="text-[11px] text-amber-200/80 mb-1 block">受注契約</label>
                    <select
                      value={linkContractId}
                      onChange={(e) => setLinkContractId(e.target.value)}
                      className="w-full bg-muted border border-border text-sm text-foreground rounded-md px-3 py-1.5"
                    >
                      <option value="">選択してください</option>
                      {linkCandidates.map((c) => (
                        <option key={c.id} value={c.id}>
                          #{c.id} {c.engineer_name} / {c.project_name}
                          {c.name_matched ? " ★件名一致" : ""}
                          {c.is_active ? "" : "（無効）"}
                          （{String(c.start_date).slice(0, 10)}〜{String(c.end_date).slice(0, 10)}）
                        </option>
                      ))}
                    </select>
                  </div>
                  <button
                    type="button"
                    disabled={!linkContractId || linkContract.isPending}
                    onClick={() => linkContract.mutate()}
                    className="px-3 py-1.5 text-xs font-medium rounded-md bg-amber-500/20 text-amber-200 border border-amber-500/40 hover:bg-amber-500/30 disabled:opacity-50"
                  >
                    {linkContract.isPending ? "紐付け中…" : "受注契約を紐付け"}
                  </button>
                </div>
              )}
            </div>
          )}

          <FieldGrid>
            <Field label="クライアント" value={data.client_name} />
            <Field label="案件" value={order.project_name} />
            <Field label="ステータス" value={
              <Badge className="text-[10px]">{data.status_display}</Badge>
            } />
            <Field label="対象年月" value={order.target_month?.slice(0, 7)} />
          </FieldGrid>
          <FieldGrid>
            <Field label="受注書番号" value={order.received_order_no || "—"} />
            <Field label="受注契約ID" value={order.client_contract_id != null ? String(order.client_contract_id) : "未紐付け"} />
            <Field label="定期区分" value={order.is_recurring ? "定期" : "単発"} />
            <Field label="作成日" value={order.created_at?.slice(0, 10)} />
          </FieldGrid>

          {/* ステータス変更 */}
          <div className="bg-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-medium text-foreground mb-2">ステータス変更</h3>
            <div className="flex items-center gap-2">
              <select
                value={selectedStatus}
                onChange={(e) => setSelectedStatus(e.target.value)}
                className="bg-muted border border-border text-sm text-foreground rounded-md px-3 py-1.5 flex-1 max-w-xs"
              >
                <option value="">選択してください</option>
                {STATUS_OPTIONS.map((s) => (
                  <option key={s.value} value={s.value}>{s.label}</option>
                ))}
              </select>
              <button
                onClick={() => { if (selectedStatus) updateStatus.mutate(selectedStatus); }}
                disabled={!selectedStatus || updateStatus.isPending}
                className="px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
              >
                変更
              </button>
            </div>
          </div>

          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-border">
              <span className="text-sm font-medium text-foreground">受注明細 ({items.length}件)</span>
            </div>
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">作業者</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">単価</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">工数</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">実績時間</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">調整</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">金額</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item, i) => (
                  <TableRow key={i} className="border-border/50">
                    <TableCell className="text-sm">{String(item.engineer_name ?? "—")}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">¥{Number(item.unit_price ?? 0).toLocaleString()}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{String(item.man_month ?? "—")}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{String(item.actual_hours ?? "—")}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{Number(item.adjustment ?? 0) !== 0 ? `¥${Number(item.adjustment).toLocaleString()}` : "—"}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums font-medium">¥{Number(item.amount ?? 0).toLocaleString()}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          {/* 備考 */}
          {order.remarks && (
            <div className="bg-card border border-border rounded-lg p-4">
              <dt className="text-[11px] text-muted-foreground font-medium mb-1">備考</dt>
              <dd className="text-sm text-foreground whitespace-pre-wrap">{order.remarks}</dd>
            </div>
          )}
        </>
      )}

      {/* 編集モーダル */}
      <FormModal
        open={showEdit}
        title="受注書の編集"
        size="xl"
        loading={editMutation.isPending}
        submitLabel="保存"
        onSubmit={() => editMutation.mutate()}
        onClose={() => setShowEdit(false)}
      >
        <div className="grid grid-cols-3 gap-4">
          <FormField label="案件名">
            <FormInput value={headerForm.project_name} onChange={(e) => setHeaderForm((p) => ({ ...p, project_name: e.target.value }))} />
          </FormField>
          <FormField label="対象年月">
            <FormInput type="month" value={headerForm.target_month} onChange={(e) => setHeaderForm((p) => ({ ...p, target_month: e.target.value }))} />
          </FormField>
          <FormField label="先方発注番号">
            <FormInput value={headerForm.client_order_number} onChange={(e) => setHeaderForm((p) => ({ ...p, client_order_number: e.target.value }))} />
          </FormField>
          <FormField label="作業期間（開始）">
            <FormInput type="date" value={headerForm.work_start} onChange={(e) => setHeaderForm((p) => ({ ...p, work_start: e.target.value }))} />
          </FormField>
          <FormField label="作業期間（終了）">
            <FormInput type="date" value={headerForm.work_end} onChange={(e) => setHeaderForm((p) => ({ ...p, work_end: e.target.value }))} />
          </FormField>
          <FormField label="支払条件">
            <FormInput value={headerForm.payment_condition} onChange={(e) => setHeaderForm((p) => ({ ...p, payment_condition: e.target.value }))} />
          </FormField>
        </div>

        <div className="grid grid-cols-2 gap-4 mt-4">
          <FormField label="レポート送付先">
            <FormInput type="email" value={headerForm.report_to_email} onChange={(e) => setHeaderForm((p) => ({ ...p, report_to_email: e.target.value }))} />
          </FormField>
          <FormField label="レポートCC（カンマ区切り）">
            <FormInput value={headerForm.report_cc_emails} onChange={(e) => setHeaderForm((p) => ({ ...p, report_cc_emails: e.target.value }))} />
          </FormField>
          <FormField label="請求送付先">
            <FormInput type="email" value={headerForm.invoice_to_email} onChange={(e) => setHeaderForm((p) => ({ ...p, invoice_to_email: e.target.value }))} />
          </FormField>
          <FormField label="請求CC（カンマ区切り）">
            <FormInput value={headerForm.invoice_cc_emails} onChange={(e) => setHeaderForm((p) => ({ ...p, invoice_cc_emails: e.target.value }))} />
          </FormField>
        </div>

        <FormField label="備考" className="mt-4">
          <textarea
            className="w-full px-3 py-2 bg-muted border border-border rounded-md text-sm text-foreground min-h-[80px]"
            value={headerForm.remarks}
            onChange={(e) => setHeaderForm((p) => ({ ...p, remarks: e.target.value }))}
          />
        </FormField>

        {itemForms.length > 0 && (
          <div className="mt-4 space-y-3">
            <span className="text-xs font-medium text-muted-foreground">明細</span>
            {itemForms.map((f, idx) => (
              <div key={f.id} className="border border-border rounded-lg p-3">
                <div className="text-xs text-muted-foreground mb-2 font-medium">{f.engineer_name || `明細 #${f.id}`}</div>
                <div className="grid grid-cols-4 gap-3">
                  <FormField label="単価">
                    <FormInput type="number" value={f.unit_price} onChange={(e) => setItemField(idx, "unit_price", e.target.value)} />
                  </FormField>
                  <FormField label="工数">
                    <FormInput type="number" step="0.1" value={f.man_month} onChange={(e) => setItemField(idx, "man_month", e.target.value)} />
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

        {editMutation.isError && (
          <p className="text-sm text-red-400 mt-2">エラー: {(editMutation.error as Error).message}</p>
        )}
      </FormModal>
    </DetailLayout>
  );
}
