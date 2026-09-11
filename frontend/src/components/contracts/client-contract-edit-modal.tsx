"use client";

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchClientContractDetail, apiPut, apiPost, apiDelete, createReceivedOrderFromContract } from "@/lib/api";
import { FormModal, FormField, FormInput, FormSelect, FormTextarea } from "@/components/ui/form-modal";
import { FormSection } from "@/components/contracts/form-section";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

const SETTLEMENT_TYPES = [
  { value: "上下割", label: "上下割" },
  { value: "中間割", label: "中間割" },
  { value: "固定時間", label: "固定時間" },
  { value: "一律割", label: "一律割" },
];

const MID_MONTH_OPTIONS = [
  { value: "按分", label: "按分" },
  { value: "全日", label: "全日" },
];

interface ClientEditForm {
  project_id: string;
  engineer_id: string;
  start_date: string;
  end_date: string;
  settlement_type: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  fixed_hours: string;
  base_rate: string;
  deduction_rate: string;
  overtime_rate: string;
  effort: string;
  mid_month_rule: string;
  billing_timing: string;
  payment_terms: string;
  currency: string;
  report_deadline_days_before: string;
  remarks: string;
  is_active: boolean;
}

/** 契約期間内の対象月（YYYY-MM）候補を生成 */
function monthsInRange(startDate: string, endDate: string): { value: string; label: string }[] {
  if (!startDate || !endDate) return [];
  const start = new Date(`${startDate.slice(0, 7)}-01T00:00:00`);
  const end = new Date(`${endDate.slice(0, 7)}-01T00:00:00`);
  if (Number.isNaN(start.getTime()) || Number.isNaN(end.getTime()) || start > end) return [];
  const out: { value: string; label: string }[] = [];
  const cur = new Date(start);
  while (cur <= end) {
    const y = cur.getFullYear();
    const m = String(cur.getMonth() + 1).padStart(2, "0");
    out.push({ value: `${y}-${m}`, label: `${y}年${m}月` });
    cur.setMonth(cur.getMonth() + 1);
  }
  return out;
}

function defaultMonthValue(startDate: string, endDate: string): string {
  const options = monthsInRange(startDate, endDate);
  if (options.length === 0) return "";
  const now = new Date();
  const current = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  if (options.some((o) => o.value === current)) return current;
  return options[options.length - 1].value;
}

/** 対象月と契約期間から target_month / work_start / work_end を算出 */
function buildWorkPeriod(yearMonth: string, contractStart: string, contractEnd: string) {
  const [y, m] = yearMonth.split("-").map(Number);
  const targetMonth = `${yearMonth}-01`;
  const lastDay = new Date(y, m, 0).getDate();
  const monthEnd = `${yearMonth}-${String(lastDay).padStart(2, "0")}`;
  const workStart = contractStart > targetMonth ? contractStart : targetMonth;
  const workEnd = contractEnd < monthEnd ? contractEnd : monthEnd;
  return { target_month: targetMonth, work_start: workStart, work_end: workEnd };
}

async function fetchFormData(): Promise<{
  projects: { value: string; label: string }[];
  engineers: { value: number; label: string }[];
}> {
  const res = await fetch("/api/v1/client-contracts/form-data", { credentials: "include" });
  if (!res.ok) throw new Error("form-data取得失敗");
  return res.json();
}

interface Props {
  contractId: number | null;
  open: boolean;
  onClose: () => void;
}

export function ClientContractEditModal({ contractId, open, onClose }: Props) {
  const qc = useQueryClient();
  const [form, setForm] = useState<ClientEditForm | null>(null);
  const [orderMonth, setOrderMonth] = useState("");
  const [extendDate, setExtendDate] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["client-contracts", String(contractId)],
    queryFn: () => fetchClientContractDetail(String(contractId)),
    enabled: open && contractId != null,
  });

  const { data: formData } = useQuery({
    queryKey: ["client-contract-form-data"],
    queryFn: fetchFormData,
    enabled: open,
    staleTime: 5 * 60 * 1000,
  });

  const c = data?.contract;

  const monthOptions = useMemo(
    () => monthsInRange(c?.start_date ?? "", c?.end_date ?? ""),
    [c?.start_date, c?.end_date]
  );

  useEffect(() => {
    if (!c || !open) return;
    setForm({
      project_id: c.project_id || "",
      engineer_id: String(c.engineer_id ?? ""),
      start_date: c.start_date || "",
      end_date: c.end_date || "",
      settlement_type: c.settlement_type || "上下割",
      lower_limit_hours: String(c.lower_limit_hours ?? ""),
      upper_limit_hours: String(c.upper_limit_hours ?? ""),
      fixed_hours: c.fixed_hours != null ? String(c.fixed_hours) : "",
      base_rate: String(c.base_rate ?? ""),
      deduction_rate: String(c.deduction_rate ?? ""),
      overtime_rate: String(c.overtime_rate ?? ""),
      effort: String(c.effort ?? "1.0"),
      mid_month_rule: c.mid_month_rule || "按分",
      billing_timing: c.billing_timing || "",
      payment_terms: c.payment_terms || "",
      currency: c.currency || "JPY",
      report_deadline_days_before: String(c.report_deadline_days_before ?? 5),
      remarks: c.remarks || "",
      is_active: c.is_active ?? true,
    });
    setOrderMonth(defaultMonthValue(c.start_date || "", c.end_date || ""));
  }, [c, open]);

  const updateMutation = useMutation({
    mutationFn: (payload: ClientEditForm) =>
      apiPut(`/api/v1/client-contracts/${contractId}`, {
        project_id: payload.project_id,
        engineer_id: Number(payload.engineer_id),
        start_date: payload.start_date,
        end_date: payload.end_date,
        settlement_type: payload.settlement_type,
        base_rate: Number(payload.base_rate),
        effort: Number(payload.effort) || 1.0,
        lower_limit_hours: Number(payload.lower_limit_hours) || 0,
        upper_limit_hours: Number(payload.upper_limit_hours) || 0,
        fixed_hours: payload.fixed_hours ? Number(payload.fixed_hours) : null,
        deduction_rate: Number(payload.deduction_rate),
        overtime_rate: Number(payload.overtime_rate),
        mid_month_rule: payload.mid_month_rule || null,
        billing_timing: payload.billing_timing || null,
        payment_terms: payload.payment_terms || null,
        currency: payload.currency || null,
        report_deadline_days_before: Number(payload.report_deadline_days_before) || 5,
        remarks: payload.remarks || null,
        is_active: payload.is_active ? "true" : "false",
      }),
    onSuccess: (_res, payload) => {
      qc.invalidateQueries({ queryKey: ["client-contracts"] });
      if (contractId != null) {
        qc.invalidateQueries({ queryKey: ["client-contracts", String(contractId)] });
      }
      // 無効化保存時はモーダルを開き続け、続けて削除できるようにする
      if (!payload.is_active) {
        toast.success("無効として保存しました。削除できます");
        return;
      }
      onClose();
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(`更新エラー: ${e.message}`),
  });

  const extendMutation = useMutation({
    mutationFn: () =>
      apiPost<{ success?: boolean; error?: string; message?: string }>(
        `/api/v1/client-contracts/${contractId}/extend`,
        { end_date: extendDate }
      ),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "延長に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["client-contracts"] });
      if (contractId != null) {
        qc.invalidateQueries({ queryKey: ["client-contracts", String(contractId)] });
      }
      setExtendDate("");
      toast.success(res.message ?? "契約を延長しました");
    },
    onError: (e: Error) => toast.error(`延長エラー: ${e.message}`),
  });

  const deleteMutation = useMutation({
    mutationFn: () => apiDelete<{ success?: boolean; error?: string; message?: string }>(
      `/api/v1/client-contracts/${contractId}`
    ),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "削除に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["client-contracts"] });
      onClose();
      toast.success(res.message ?? "削除しました");
    },
    onError: (e: Error) => toast.error(`削除エラー: ${e.message}`),
  });

  const createOrderMutation = useMutation({
    mutationFn: async () => {
      if (contractId == null || !c?.start_date || !c?.end_date || !orderMonth) {
        throw new Error("対象月または契約期間が不正です");
      }
      const period = buildWorkPeriod(orderMonth, c.start_date, c.end_date);
      if (period.work_start > period.work_end) {
        throw new Error("対象月が契約期間外です");
      }
      return createReceivedOrderFromContract({
        client_contract_id: contractId,
        ...period,
      });
    },
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["received-orders"] });
      toast.success(`受注書 ${res.received_order_no} を作成しました`);
    },
    onError: (e: Error) => toast.error(`受注書作成エラー: ${e.message}`),
  });

  const setField = (key: keyof ClientEditForm, value: string | boolean) =>
    setForm((prev) => (prev ? { ...prev, [key]: value } : prev));

  if (!open || contractId == null) return null;

  const locked = !!data?.is_locked;
  /** 承諾済み受注が無い契約は削除可（発注契約と同じ） */
  const canDelete = !!c && !locked;
  const canCreateOrder = !!c && c.is_active !== false && monthOptions.length > 0 && !!orderMonth;

  return (
    <FormModal
      open={open}
      title={`受注契約 #${contractId} の編集`}
      size="xl"
      loading={
        updateMutation.isPending
        || deleteMutation.isPending
        || createOrderMutation.isPending
        || isLoading
        || !form
      }
      submitLabel={locked ? "編集不可" : "保存"}
      onSubmit={() => {
        if (locked || !form) return;
        updateMutation.mutate(form);
      }}
      onClose={onClose}
      footerStart={
        <div className="flex items-center gap-2 flex-wrap">
          {canDelete ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="border-red-500/40 text-red-400 hover:bg-red-500/10 hover:text-red-300"
              disabled={deleteMutation.isPending || updateMutation.isPending || createOrderMutation.isPending}
              onClick={() => {
                if (!confirm(`受注契約 #${contractId} を削除しますか？この操作は取り消せません。`)) return;
                deleteMutation.mutate();
              }}
            >
              {deleteMutation.isPending ? "削除中…" : "削除"}
            </Button>
          ) : locked ? (
            <p className="text-[11px] text-muted-foreground self-center max-w-[14rem]">
              承諾済みの受注書があるため削除できません
            </p>
          ) : null}
        </div>
      }
    >
      {locked && (
        <div className="text-sm text-amber-400 bg-amber-500/10 border border-amber-500/30 rounded-md px-3 py-2 space-y-2">
          <p>🔒 承諾済みの受注書があるため、精算条件等の編集はできません</p>
          <p className="text-xs text-amber-400/80">
            契約更改（期間延長）のみ、既存の受注書に影響しないため常に可能です。現在の終了日: {c?.end_date || "-"}
          </p>
          <div className="flex items-center gap-2 flex-wrap">
            <input
              type="date"
              value={extendDate}
              onChange={(e) => setExtendDate(e.target.value)}
              min={c?.end_date || undefined}
              className="bg-background border border-amber-500/30 rounded-md px-2 py-1 text-xs text-foreground"
            />
            <Button
              type="button"
              size="sm"
              disabled={!extendDate || extendMutation.isPending}
              onClick={() => {
                if (!confirm(`終了日を ${extendDate} に延長しますか？`)) return;
                extendMutation.mutate();
              }}
            >
              {extendMutation.isPending ? "延長中…" : "契約を延長"}
            </Button>
          </div>
        </div>
      )}

      {!form || isLoading ? (
        <div className="space-y-3">
          {[...Array(3)].map((_, i) => (
            <div key={i} className="h-16 bg-muted/50 rounded animate-pulse" />
          ))}
        </div>
      ) : (
        <>
          <fieldset disabled={locked} className="space-y-3 disabled:opacity-60">
            <FormSection title="① 基本情報">
              <div className="grid grid-cols-2 gap-3">
                <FormField label="案件">
                  <FormInput value={c?.project_name ?? form.project_id} disabled />
                  <p className="text-[10px] text-muted-foreground mt-1">
                    クライアント: {c?.client_name ?? "—"}（案件から導出）
                  </p>
                </FormField>
                <FormField label="技術者" required>
                  <FormSelect
                    options={(formData?.engineers ?? []).map((e) => ({
                      value: String(e.value),
                      label: e.label,
                    }))}
                    value={form.engineer_id}
                    onChange={(e) => setField("engineer_id", e.target.value)}
                  />
                </FormField>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <FormField label="開始日" required>
                  <FormInput
                    type="date"
                    value={form.start_date}
                    onChange={(e) => setField("start_date", e.target.value)}
                  />
                </FormField>
                <FormField label="終了日" required>
                  <FormInput
                    type="date"
                    value={form.end_date}
                    onChange={(e) => setField("end_date", e.target.value)}
                  />
                </FormField>
              </div>
              <label className="flex items-center gap-2 text-sm text-foreground">
                <input
                  type="checkbox"
                  checked={form.is_active}
                  onChange={(e) => setField("is_active", e.target.checked)}
                />
                有効
              </label>
            </FormSection>

            <FormSection title="② 精算・請求ルール">
              <div className="grid grid-cols-4 gap-3">
                <FormField label="精算方式" required>
                  <FormSelect
                    options={SETTLEMENT_TYPES}
                    value={form.settlement_type}
                    onChange={(e) => setField("settlement_type", e.target.value)}
                  />
                </FormField>
                <FormField label="単金" required>
                  <FormInput
                    type="number"
                    value={form.base_rate}
                    onChange={(e) => setField("base_rate", e.target.value)}
                  />
                </FormField>
                <FormField label="工数">
                  <FormInput
                    type="number"
                    step="0.1"
                    value={form.effort}
                    onChange={(e) => setField("effort", e.target.value)}
                  />
                </FormField>
                <FormField label="通貨">
                  <FormInput
                    value={form.currency}
                    onChange={(e) => setField("currency", e.target.value)}
                  />
                </FormField>
              </div>
              <div className="grid grid-cols-4 gap-3">
                <FormField label="下限時間">
                  <FormInput
                    type="number"
                    value={form.lower_limit_hours}
                    onChange={(e) => setField("lower_limit_hours", e.target.value)}
                  />
                </FormField>
                <FormField label="上限時間">
                  <FormInput
                    type="number"
                    value={form.upper_limit_hours}
                    onChange={(e) => setField("upper_limit_hours", e.target.value)}
                  />
                </FormField>
                <FormField label="固定時間">
                  <FormInput
                    type="number"
                    value={form.fixed_hours}
                    onChange={(e) => setField("fixed_hours", e.target.value)}
                  />
                </FormField>
                <FormField label="月中ルール">
                  <FormSelect
                    options={MID_MONTH_OPTIONS}
                    value={form.mid_month_rule}
                    onChange={(e) => setField("mid_month_rule", e.target.value)}
                  />
                </FormField>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <FormField label="控除単価">
                  <FormInput
                    type="number"
                    value={form.deduction_rate}
                    onChange={(e) => setField("deduction_rate", e.target.value)}
                  />
                </FormField>
                <FormField label="超過単価">
                  <FormInput
                    type="number"
                    value={form.overtime_rate}
                    onChange={(e) => setField("overtime_rate", e.target.value)}
                  />
                </FormField>
                <FormField label="請求タイミング">
                  <FormInput
                    value={form.billing_timing}
                    onChange={(e) => setField("billing_timing", e.target.value)}
                  />
                </FormField>
                <FormField label="支払条件">
                  <FormInput
                    value={form.payment_terms}
                    onChange={(e) => setField("payment_terms", e.target.value)}
                  />
                </FormField>
              </div>
            </FormSection>

            <FormSection title="③ 稼働報告期限">
              <FormField label="提出期限（月末N営業日前）">
                <FormInput
                  type="number"
                  value={form.report_deadline_days_before}
                  onChange={(e) => setField("report_deadline_days_before", e.target.value)}
                />
                <p className="text-[10px] text-muted-foreground mt-1">
                  案件側に提出期限値がある場合は案件設定が優先されます
                </p>
              </FormField>
            </FormSection>

            <FormSection title="④ 備考" defaultOpen={false}>
              <FormField label="備考">
                <FormTextarea
                  value={form.remarks}
                  onChange={(e) => setField("remarks", e.target.value)}
                />
              </FormField>
            </FormSection>
          </fieldset>

          {/* 承諾済みでも受注書作成は可能（契約スナップショット） */}
          <FormSection title="⑤ 受注書作成">
            <div className="flex flex-wrap items-end gap-3">
              <FormField label="対象月">
                <FormSelect
                  options={monthOptions}
                  value={orderMonth}
                  onChange={(e) => setOrderMonth(e.target.value)}
                />
              </FormField>
              <Button
                type="button"
                size="sm"
                disabled={!canCreateOrder || createOrderMutation.isPending}
                onClick={() => {
                  const label = monthOptions.find((o) => o.value === orderMonth)?.label ?? orderMonth;
                  if (!confirm(`${label}の受注書を作成しますか？`)) return;
                  createOrderMutation.mutate();
                }}
              >
                {createOrderMutation.isPending ? "作成中…" : "受注書を作成"}
              </Button>
            </div>
            <p className="text-[11px] text-muted-foreground mt-2">
              契約の精算条件をスナップショットした受注書を作成します。同一契約・同一月が既にある場合は作成できません。
            </p>
          </FormSection>
        </>
      )}

      {updateMutation.isError && (
        <p className="text-sm text-red-400">
          エラー: {(updateMutation.error as Error).message}
        </p>
      )}
    </FormModal>
  );
}
