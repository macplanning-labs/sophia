"use client";

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchProjectDetail, fetchProjectWizardFormData, updateProject, deleteProject } from "@/lib/api";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

const REPORT_DEADLINE_TYPES = [
  { value: "RELATIVE", label: "月末相対（N営業日前）" },
  { value: "FIXED_DAY", label: "当月固定日" },
];

const REPORT_DEADLINE_HOLIDAY_RULES = [
  { value: "PREVIOUS_BUSINESS_DAY", label: "前営業日" },
  { value: "NEXT_BUSINESS_DAY", label: "翌営業日" },
];

interface EditForm {
  client_id: string;
  name: string;
  edi_project_alias: string;
  is_active: boolean;
  report_deadline_type: string;
  report_deadline_value: string;
  report_deadline_holiday_rule: string;
}

interface Props {
  projectId: string | null;
  open: boolean;
  onClose: () => void;
}

function derivePeriod(
  clientContracts: { start_date: string; end_date: string }[],
  partnerContracts: { start_date: string; end_date: string }[]
): { start: string; end: string } | null {
  const dates = [...clientContracts, ...partnerContracts];
  if (dates.length === 0) return null;
  const starts = dates.map((d) => d.start_date).sort();
  const ends = dates.map((d) => d.end_date).sort();
  return { start: starts[0], end: ends[ends.length - 1] };
}

export function ProjectEditModal({ projectId, open, onClose }: Props) {
  const qc = useQueryClient();
  const [form, setForm] = useState<EditForm | null>(null);

  const { data: detail, isLoading: detailLoading } = useQuery({
    queryKey: ["projects", projectId],
    queryFn: () => fetchProjectDetail(projectId!),
    enabled: open && !!projectId,
  });

  const { data: formData } = useQuery({
    queryKey: ["projects-wizard-form-data"],
    queryFn: fetchProjectWizardFormData,
    enabled: open,
    staleTime: 5 * 60 * 1000,
  });

  const project = detail?.project;
  const clientContracts = detail?.client_contracts ?? [];
  const partnerContracts = detail?.partner_contracts ?? [];
  const period = useMemo(
    () => derivePeriod(clientContracts, partnerContracts),
    [clientContracts, partnerContracts]
  );

  const clientOptions = useMemo(() => {
    const list = (formData?.clients ?? []) as { value: number; label: string }[];
    return list.map((c) => ({ value: String(c.value), label: c.label }));
  }, [formData]);

  useEffect(() => {
    if (!project || !open) return;
    setForm({
      client_id: String(project.client_id),
      name: project.name,
      edi_project_alias: project.edi_project_alias ?? "",
      is_active: project.is_active,
      report_deadline_type: project.report_deadline_type ?? "RELATIVE",
      report_deadline_value:
        project.report_deadline_value != null ? String(project.report_deadline_value) : "",
      report_deadline_holiday_rule: project.report_deadline_holiday_rule ?? "PREVIOUS_BUSINESS_DAY",
    });
  }, [project, open]);

  const updateMutation = useMutation({
    mutationFn: (f: EditForm) =>
      updateProject(projectId!, {
        client_id: Number(f.client_id),
        name: f.name,
        description: project?.description ?? "",
        is_active: f.is_active,
        edi_project_alias: f.edi_project_alias.trim(),
        report_deadline_type: f.report_deadline_type,
        report_deadline_value: f.report_deadline_value ? Number(f.report_deadline_value) : null,
        report_deadline_holiday_rule:
          f.report_deadline_type === "FIXED_DAY" ? f.report_deadline_holiday_rule || null : null,
        report_request_day: project?.report_request_day ?? null,
      }),
    onSuccess: (_res, payload) => {
      qc.invalidateQueries({ queryKey: ["projects"] });
      if (projectId) qc.invalidateQueries({ queryKey: ["projects", projectId] });
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

  const deleteMutation = useMutation({
    mutationFn: () => deleteProject(projectId!),
    onSuccess: (res) => {
      if (res.success === false) {
        toast.error(res.error ?? "削除に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["projects"] });
      onClose();
      toast.success(res.message ?? "削除しました");
    },
    onError: (e: Error) => toast.error(`削除エラー: ${e.message}`),
  });

  if (!open || !projectId) return null;

  const canDelete = !!project && project.is_active === false;

  return (
    <FormModal
      open={open}
      title="案件の編集"
      size="lg"
      loading={updateMutation.isPending || deleteMutation.isPending || detailLoading || !form}
      onSubmit={() => form && updateMutation.mutate(form)}
      onClose={onClose}
      footerStart={
        canDelete ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="border-red-500/40 text-red-400 hover:bg-red-500/10 hover:text-red-300"
            disabled={deleteMutation.isPending || updateMutation.isPending}
            onClick={() => {
              if (!confirm(`案件「${project?.name ?? projectId}」を削除しますか？この操作は取り消せません。`)) return;
              deleteMutation.mutate();
            }}
          >
            {deleteMutation.isPending ? "削除中…" : "削除"}
          </Button>
        ) : project?.is_active ? (
          <p className="text-[11px] text-muted-foreground self-center max-w-[14rem]">
            削除するには先に「有効」を外して保存してください
          </p>
        ) : undefined
      }
    >
      {!form || detailLoading ? (
        <div className="space-y-3 py-2">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />
          ))}
        </div>
      ) : (
        <>
          <FormField label="案件ID">
            <FormInput value={projectId} disabled className="opacity-70 cursor-not-allowed" />
          </FormField>

          <FormField label="クライアント" required>
            <FormSelect
              options={
                clientOptions.length > 0
                  ? clientOptions
                  : [{ value: form.client_id, label: project?.client_name ?? form.client_id }]
              }
              value={form.client_id}
              onChange={(e) => setForm({ ...form, client_id: e.target.value })}
            />
          </FormField>

          <FormField label="案件名" required>
            <FormInput
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </FormField>

          <FormField label="EDI案件別名">
            <FormInput
              value={form.edi_project_alias}
              onChange={(e) => setForm({ ...form, edi_project_alias: e.target.value })}
              placeholder="例: AJIS（注文書の案件名が正式名と違う場合）"
            />
            <p className="text-[11px] text-muted-foreground mt-1">
              EDI注文書の案件名がマスタ正式名と異なるときに設定します。空欄可。
            </p>
          </FormField>

          <div className="grid grid-cols-2 gap-4">
            <FormField label="期間（開始日）">
              <FormInput
                value={period?.start ?? ""}
                disabled
                placeholder="契約未登録"
                className="opacity-70 cursor-not-allowed"
              />
            </FormField>
            <FormField label="期間（終了日）">
              <FormInput
                value={period?.end ?? ""}
                disabled
                placeholder="契約未登録"
                className="opacity-70 cursor-not-allowed"
              />
            </FormField>
          </div>
          <p className="text-[11px] text-muted-foreground -mt-2">
            期間は紐づく契約の最早開始〜最遅終了を表示しています。編集は契約画面で行います。
          </p>

          <div className="border-t border-border pt-3 space-y-3">
            <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              稼働報告書設定
            </p>
            <div className="grid grid-cols-2 gap-4">
              <FormField label="提出期限の種類" required>
                <FormSelect
                  options={REPORT_DEADLINE_TYPES}
                  value={form.report_deadline_type}
                  onChange={(e) => setForm({ ...form, report_deadline_type: e.target.value })}
                />
              </FormField>
              <FormField label="提出期限の値">
                <FormInput
                  type="number"
                  value={form.report_deadline_value}
                  onChange={(e) => setForm({ ...form, report_deadline_value: e.target.value })}
                  placeholder="空欄なら受注契約側の設定に従う"
                />
              </FormField>
            </div>
            {form.report_deadline_type === "FIXED_DAY" && (
              <FormField label="非営業日の調整">
                <FormSelect
                  options={REPORT_DEADLINE_HOLIDAY_RULES}
                  value={form.report_deadline_holiday_rule}
                  onChange={(e) =>
                    setForm({ ...form, report_deadline_holiday_rule: e.target.value })
                  }
                />
              </FormField>
            )}
          </div>

          <label className="flex items-center gap-2 text-sm text-foreground">
            <input
              type="checkbox"
              checked={form.is_active}
              onChange={(e) => setForm({ ...form, is_active: e.target.checked })}
            />
            有効
          </label>

          <div className="flex flex-wrap gap-2 pt-1 text-xs">
            <a
              href={`/orders?project_id=${encodeURIComponent(projectId)}`}
              className="text-sky-400 hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              発注書一覧 →
            </a>
            <a
              href={`/received-orders?project_id=${encodeURIComponent(projectId)}`}
              className="text-sky-400 hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              受注書一覧 →
            </a>
            <a
              href={`/partner-contracts?project_id=${encodeURIComponent(projectId)}`}
              className="text-sky-400 hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              パートナー契約 →
            </a>
            <a
              href={`/client-contracts?project_id=${encodeURIComponent(projectId)}`}
              className="text-sky-400 hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              クライアント契約 →
            </a>
          </div>

          {updateMutation.isError && (
            <p className="text-sm text-red-400">
              エラー: {(updateMutation.error as Error).message}
            </p>
          )}
        </>
      )}
    </FormModal>
  );
}
