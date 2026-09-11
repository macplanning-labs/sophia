"use client";

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  fetchPartnerContractDetail,
  fetchPartnerContractFormData,
  apiPut,
  apiPost,
  apiDelete,
} from "@/lib/api";
import { FormModal, FormField, FormInput, FormSelect, FormTextarea } from "@/components/ui/form-modal";
import { FormSection } from "@/components/contracts/form-section";
import {
  WorkLocationField,
  DEFAULT_WORK_LOCATION,
} from "@/components/contracts/work-location-field";
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

export interface PartnerEditForm {
  project_id: string;
  engineer_id: string;
  partner_id: string;
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
  kou_responsible: string;
  kou_contact: string;
  otsu_responsible: string;
  otsu_contact: string;
  work_responsible: string;
  deliverable_text: string;
  contract_items: string;
  work_location: string;
  payment_condition: string;
  remarks: string;
  order_create_deadline_day: string;
  order_approve_deadline_days_before: string;
  report_upload_deadline_days_before: string;
  invoice_create_deadline_day: string;
  invoice_approve_deadline_day: string;
}

interface Props {
  contractId: number | null;
  open: boolean;
  onClose: () => void;
}

export function PartnerContractEditModal({ contractId, open, onClose }: Props) {
  const qc = useQueryClient();
  const [form, setForm] = useState<PartnerEditForm | null>(null);
  const [extendDate, setExtendDate] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["partner-contracts", String(contractId)],
    queryFn: () => fetchPartnerContractDetail(String(contractId)),
    enabled: open && contractId != null,
  });

  const { data: formData } = useQuery({
    queryKey: ["partner-contract-form-data"],
    queryFn: fetchPartnerContractFormData,
    enabled: open,
    staleTime: 5 * 60 * 1000,
  });

  useEffect(() => {
    if (!data || !open) return;
    setForm({
      project_id: data.project_id || "",
      engineer_id: String(data.engineer_id ?? ""),
      partner_id: data.partner_id || "",
      start_date: data.start_date || "",
      end_date: data.end_date || "",
      settlement_type: data.settlement_type || "上下割",
      lower_limit_hours: String(data.lower_limit_hours ?? ""),
      upper_limit_hours: String(data.upper_limit_hours ?? ""),
      fixed_hours: data.fixed_hours != null ? String(data.fixed_hours) : "",
      base_rate: String(data.base_rate ?? ""),
      deduction_rate: String(data.deduction_rate ?? ""),
      overtime_rate: String(data.overtime_rate ?? ""),
      effort: String(data.effort ?? "1.0"),
      mid_month_rule: data.mid_month_rule || "按分",
      kou_responsible: data.kou_responsible || "",
      kou_contact: data.kou_contact || "",
      otsu_responsible: data.otsu_responsible || "",
      otsu_contact: data.otsu_contact || "",
      work_responsible: data.work_responsible || "",
      deliverable_text: data.deliverable_text || "",
      contract_items: data.contract_items || "",
      work_location: data.work_location?.trim() || DEFAULT_WORK_LOCATION,
      payment_condition: data.payment_condition || "",
      remarks: data.remarks || "",
      order_create_deadline_day: String(data.order_create_deadline_day ?? 15),
      order_approve_deadline_days_before: String(data.order_approve_deadline_days_before ?? 0),
      report_upload_deadline_days_before: String(data.report_upload_deadline_days_before ?? 2),
      invoice_create_deadline_day: String(data.invoice_create_deadline_day ?? 1),
      invoice_approve_deadline_day: String(data.invoice_approve_deadline_day ?? 10),
    });
  }, [data, open]);

  const updateMutation = useMutation({
    mutationFn: (payload: PartnerEditForm) =>
      apiPut(`/api/v1/partner-contracts/${contractId}`, {
        ...payload,
        engineer_id: Number(payload.engineer_id),
        base_rate: Number(payload.base_rate),
        deduction_rate: Number(payload.deduction_rate),
        overtime_rate: Number(payload.overtime_rate),
        effort: payload.effort,
        lower_limit_hours: payload.lower_limit_hours,
        upper_limit_hours: payload.upper_limit_hours,
        fixed_hours: payload.fixed_hours || null,
        order_create_deadline_day: Number(payload.order_create_deadline_day),
        order_approve_deadline_days_before: Number(payload.order_approve_deadline_days_before),
        report_upload_deadline_days_before: Number(payload.report_upload_deadline_days_before),
        invoice_create_deadline_day: Number(payload.invoice_create_deadline_day),
        invoice_approve_deadline_day: Number(payload.invoice_approve_deadline_day),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["partner-contracts"] });
      if (contractId != null) {
        qc.invalidateQueries({ queryKey: ["partner-contracts", String(contractId)] });
      }
      onClose();
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(`更新エラー: ${e.message}`),
  });

  const extendMutation = useMutation({
    mutationFn: () =>
      apiPost<{ success?: boolean; error?: string; message?: string }>(
        `/api/v1/partner-contracts/${contractId}/extend`,
        { end_date: extendDate }
      ),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "延長に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["partner-contracts"] });
      if (contractId != null) {
        qc.invalidateQueries({ queryKey: ["partner-contracts", String(contractId)] });
      }
      setExtendDate("");
      toast.success(res.message ?? "契約を延長しました");
    },
    onError: (e: Error) => toast.error(`延長エラー: ${e.message}`),
  });

  const deleteMutation = useMutation({
    mutationFn: () =>
      apiDelete<{ success?: boolean; error?: string; message?: string }>(
        `/api/v1/partner-contracts/${contractId}`
      ),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "削除に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["partner-contracts"] });
      onClose();
      toast.success(res.message ?? "削除しました");
    },
    onError: (e: Error) => toast.error(`削除エラー: ${e.message}`),
  });

  const setField = (key: keyof PartnerEditForm, value: string) =>
    setForm((prev) => (prev ? { ...prev, [key]: value } : prev));

  if (!open || contractId == null) return null;

  const locked = !!data?.is_locked;
  const canDelete = !locked;
  const workSuggestions = ((formData?.work_locations ?? []) as { label: string }[]).map(
    (w) => w.label
  );

  return (
    <FormModal
      open={open}
      title={`発注契約 #${contractId} の編集`}
      size="xl"
      loading={updateMutation.isPending || deleteMutation.isPending || isLoading || !form}
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
              disabled={deleteMutation.isPending || updateMutation.isPending}
              onClick={() => {
                if (!confirm(`発注契約 #${contractId} を削除しますか？この操作は取り消せません。`)) return;
                deleteMutation.mutate();
              }}
            >
              {deleteMutation.isPending ? "削除中…" : "削除"}
            </Button>
          ) : (
            <p className="text-[11px] text-muted-foreground self-center max-w-[14rem]">
              承諾済みの発注書があるため削除できません
            </p>
          )}
        </div>
      }
    >
      {locked && (
        <div className="text-sm text-amber-400 bg-amber-500/10 border border-amber-500/30 rounded-md px-3 py-2 space-y-2">
          <p>🔒 承諾済みの発注書があるため、精算条件等の編集はできません</p>
          <p className="text-xs text-amber-400/80">
            契約更改（期間延長）のみ、既存の発注書に影響しないため常に可能です。現在の終了日: {data?.end_date || "-"}
          </p>
          <div className="flex items-center gap-2 flex-wrap">
            <input
              type="date"
              value={extendDate}
              onChange={(e) => setExtendDate(e.target.value)}
              min={data?.end_date || undefined}
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
          {[...Array(4)].map((_, i) => (
            <div key={i} className="h-16 bg-muted/50 rounded animate-pulse" />
          ))}
        </div>
      ) : (
        <fieldset disabled={locked} className="space-y-3 disabled:opacity-60">
          <FormSection title="① 基本情報">
            <div className="grid grid-cols-3 gap-3">
              <FormField label="案件" required>
                <FormSelect
                  options={formData?.projects ?? []}
                  value={form.project_id}
                  onChange={(e) => setField("project_id", e.target.value)}
                />
              </FormField>
              <FormField label="パートナー" required>
                <FormSelect
                  options={formData?.partners ?? []}
                  value={form.partner_id}
                  onChange={(e) => setField("partner_id", e.target.value)}
                />
              </FormField>
              <FormField label="技術者" required>
                <FormSelect
                  options={(formData?.engineers ?? []).map(
                    (e: { value: number; label: string }) => ({
                      value: String(e.value),
                      label: e.label,
                    })
                  )}
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
          </FormSection>

          <FormSection title="② 精算ルール">
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
              <FormField label="月中ルール">
                <FormSelect
                  options={MID_MONTH_OPTIONS}
                  value={form.mid_month_rule}
                  onChange={(e) => setField("mid_month_rule", e.target.value)}
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
              <div />
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
            </div>
          </FormSection>

          <FormSection title="③ 契約書関連" defaultOpen={false}>
            <div className="grid grid-cols-2 gap-3">
              <FormField label="甲 責任者">
                <FormInput
                  value={form.kou_responsible}
                  onChange={(e) => setField("kou_responsible", e.target.value)}
                />
              </FormField>
              <FormField label="甲 担当者">
                <FormInput
                  value={form.kou_contact}
                  onChange={(e) => setField("kou_contact", e.target.value)}
                />
              </FormField>
              <FormField label="乙 責任者">
                <FormInput
                  value={form.otsu_responsible}
                  onChange={(e) => setField("otsu_responsible", e.target.value)}
                />
              </FormField>
              <FormField label="乙 担当者">
                <FormInput
                  value={form.otsu_contact}
                  onChange={(e) => setField("otsu_contact", e.target.value)}
                />
              </FormField>
            </div>
            <FormField label="作業責任者">
              <FormInput
                value={form.work_responsible}
                onChange={(e) => setField("work_responsible", e.target.value)}
              />
            </FormField>
            <FormField label="成果物">
              <FormTextarea
                value={form.deliverable_text}
                onChange={(e) => setField("deliverable_text", e.target.value)}
              />
            </FormField>
            <FormField label="契約条件">
              <FormTextarea
                value={form.contract_items}
                onChange={(e) => setField("contract_items", e.target.value)}
              />
            </FormField>
          </FormSection>

          <FormSection title="④ 勤務条件">
            <WorkLocationField
              value={form.work_location}
              onChange={(v) => setField("work_location", v)}
              suggestions={workSuggestions}
            />
            <FormField label="支払条件">
              <FormInput
                value={form.payment_condition}
                onChange={(e) => setField("payment_condition", e.target.value)}
                placeholder="毎月末日締め翌月末日払い（税別）"
              />
            </FormField>
          </FormSection>

          <FormSection title="⑤ リマインド・期限設定" defaultOpen={false}>
            <div className="grid grid-cols-3 gap-3">
              <FormField label="発注書作成期限（前月N日）">
                <FormInput
                  type="number"
                  value={form.order_create_deadline_day}
                  onChange={(e) => setField("order_create_deadline_day", e.target.value)}
                />
              </FormField>
              <FormField label="承諾期限（前月末N営業日前）">
                <FormInput
                  type="number"
                  value={form.order_approve_deadline_days_before}
                  onChange={(e) =>
                    setField("order_approve_deadline_days_before", e.target.value)
                  }
                />
              </FormField>
              <FormField label="報告提出期限（当月末N営業日前）">
                <FormInput
                  type="number"
                  value={form.report_upload_deadline_days_before}
                  onChange={(e) =>
                    setField("report_upload_deadline_days_before", e.target.value)
                  }
                />
              </FormField>
              <FormField label="請求書作成期限（翌月N日）">
                <FormInput
                  type="number"
                  value={form.invoice_create_deadline_day}
                  onChange={(e) => setField("invoice_create_deadline_day", e.target.value)}
                />
              </FormField>
              <FormField label="請求承諾期限（翌月N日）">
                <FormInput
                  type="number"
                  value={form.invoice_approve_deadline_day}
                  onChange={(e) => setField("invoice_approve_deadline_day", e.target.value)}
                />
              </FormField>
            </div>
          </FormSection>

          <FormSection title="⑥ 備考" defaultOpen={false}>
            <FormField label="備考">
              <FormTextarea
                value={form.remarks}
                onChange={(e) => setField("remarks", e.target.value)}
              />
            </FormField>
          </FormSection>
        </fieldset>
      )}

      {updateMutation.isError && (
        <p className="text-sm text-red-400">
          エラー: {(updateMutation.error as Error).message}
        </p>
      )}
    </FormModal>
  );
}
