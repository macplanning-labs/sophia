"use client";

import { useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, useEffect } from "react";
import { fetchEmployeeDetail, apiPut, apiDelete } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";

const EMPLOYMENT_TYPES = [
  { value: "REGULAR", label: "正社員" },
  { value: "CONTRACT", label: "契約社員" },
  { value: "PART_TIME", label: "パートタイム" },
  { value: "TEMPORARY", label: "派遣" },
];

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
  onDeleted?: () => void;
}

export default function EmployeeDetailPage({ id: idProp, embedded = false, onDeleted }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const router = useRouter();
  const qc = useQueryClient();

  const { data, isLoading } = useQuery({
    queryKey: ["employees", id],
    queryFn: () => fetchEmployeeDetail(id),
    enabled: !!id,
  });

  // ── 削除 ──
  const [deleteOpen, setDeleteOpen] = useState(false);
  const deleteMutation = useMutation({
    mutationFn: () => apiDelete(`/api/v1/employees/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["employees"] });
      if (embedded) {
        onDeleted?.();
      } else {
        router.push("/employees");
      }
    },
    onError: (e: Error) => toast.error(`削除に失敗しました: ${e.message}`),
  });

  // ── 編集モーダル ──
  const [editOpen, setEditOpen] = useState(false);
  const [form, setForm] = useState({
    employee_id: "",
    last_name: "",
    first_name: "",
    last_name_kana: "",
    first_name_kana: "",
    employment_type: "REGULAR",
    email: "",
    birth_date: "",
    hire_date: "",
    base_salary: "",
    position_allowance: "",
    housing_allowance: "",
    commuting_allowance: "",
    standard_monthly_hours: "",
    standard_remuneration: "",
    dependents_count: "",
  });

  useEffect(() => {
    if (data && editOpen) {
      setForm({
        employee_id: data.employee_id ?? "",
        last_name: data.last_name ?? "",
        first_name: data.first_name ?? "",
        last_name_kana: data.last_name_kana ?? "",
        first_name_kana: data.first_name_kana ?? "",
        employment_type: data.employment_type ?? "REGULAR",
        email: data.email ?? "",
        birth_date: data.birth_date ?? "",
        hire_date: data.hire_date ?? "",
        base_salary: String(data.base_salary ?? ""),
        position_allowance: String(data.position_allowance ?? ""),
        housing_allowance: String(data.housing_allowance ?? ""),
        commuting_allowance: String(data.commuting_allowance ?? ""),
        standard_monthly_hours: String(data.standard_monthly_hours ?? ""),
        standard_remuneration: String(data.standard_remuneration ?? ""),
        dependents_count: String(data.dependents_count ?? ""),
      });
    }
  }, [data, editOpen]);

  const updateMutation = useMutation({
    mutationFn: (payload: typeof form) =>
      apiPut(`/api/v1/employees/${id}`, {
        ...payload,
        base_salary: Number(payload.base_salary) || 0,
        position_allowance: Number(payload.position_allowance) || 0,
        housing_allowance: Number(payload.housing_allowance) || 0,
        commuting_allowance: Number(payload.commuting_allowance) || 0,
        standard_monthly_hours: Number(payload.standard_monthly_hours) || 160,
        standard_remuneration: Number(payload.standard_remuneration) || 0,
        dependents_count: Number(payload.dependents_count) || 0,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["employees", id] });
      qc.invalidateQueries({ queryKey: ["employees"] });
      setEditOpen(false);
    },
  });

  const setField = (key: string, value: string) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  return (
    <DetailLayout title={`社員 #${id}`} icon="👤" backHref="/employees" backLabel="一覧に戻る" isLoading={isLoading} embedded={embedded}>
      {data && (
        <>
          {/* アクションバー */}
          <div className="flex items-center gap-2 mb-4 p-3 bg-card border border-border rounded-lg">
            <Button
              variant="outline" size="sm"
              className="border-border text-foreground gap-1"
              onClick={() => setEditOpen(true)}
            >
              <Pencil className="w-3.5 h-3.5" /> 編集
            </Button>
            <Button
              variant="outline" size="sm"
              className="border-red-700/50 text-red-400 hover:text-red-300 gap-1 ml-auto"
              onClick={() => setDeleteOpen(true)}
            >
              <Trash2 className="w-3.5 h-3.5" /> 削除
            </Button>
          </div>

          <FieldGrid>
            <Field label="社員コード" value={data.employee_id} />
            <Field label="氏名" value={`${data.last_name} ${data.first_name}`} />
            <Field label="フリガナ" value={`${data.last_name_kana ?? ""} ${data.first_name_kana ?? ""}`} />
            <Field label="在籍状態" value={
              data.is_active
                ? <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">在籍</Badge>
                : <Badge variant="outline" className="border-red-500/30 text-red-400 text-[10px]">退職</Badge>
            } />
          </FieldGrid>
          <FieldGrid>
            <Field label="雇用形態" value={data.employment_type} />
            <Field label="メール" value={data.email || "—"} />
            <Field label="入社日" value={data.hire_date || "—"} />
            <Field label="生年月日" value={data.birth_date || "—"} />
          </FieldGrid>
          <FieldGrid>
            <Field label="基本給" value={`¥${Number(data.base_salary ?? 0).toLocaleString()}`} />
            <Field label="役職手当" value={`¥${Number(data.position_allowance ?? 0).toLocaleString()}`} />
            <Field label="住宅手当" value={`¥${Number(data.housing_allowance ?? 0).toLocaleString()}`} />
            <Field label="通勤手当" value={`¥${Number(data.commuting_allowance ?? 0).toLocaleString()}`} />
          </FieldGrid>
          <FieldGrid>
            <Field label="標準月額報酬" value={`¥${Number(data.standard_remuneration ?? 0).toLocaleString()}`} />
            <Field label="標準月間時間" value={`${data.standard_monthly_hours ?? 160}h`} />
            <Field label="扶養人数" value={`${data.dependents_count ?? 0}人`} />
          </FieldGrid>

          {/* 編集モーダル */}
          <FormModal
            open={editOpen}
            title={`社員 #${id} 編集`}
            size="lg"
            loading={updateMutation.isPending}
            onSubmit={() => updateMutation.mutate(form)}
            onClose={() => setEditOpen(false)}
          >
            <div className="grid grid-cols-3 gap-4">
              <FormField label="社員コード" required>
                <FormInput value={form.employee_id} onChange={(e) => setField("employee_id", e.target.value)} />
              </FormField>
              <FormField label="姓" required>
                <FormInput value={form.last_name} onChange={(e) => setField("last_name", e.target.value)} />
              </FormField>
              <FormField label="名" required>
                <FormInput value={form.first_name} onChange={(e) => setField("first_name", e.target.value)} />
              </FormField>
            </div>
            <div className="grid grid-cols-3 gap-4">
              <FormField label="フリガナ（姓）">
                <FormInput value={form.last_name_kana} onChange={(e) => setField("last_name_kana", e.target.value)} />
              </FormField>
              <FormField label="フリガナ（名）">
                <FormInput value={form.first_name_kana} onChange={(e) => setField("first_name_kana", e.target.value)} />
              </FormField>
              <FormField label="雇用形態">
                <FormSelect options={EMPLOYMENT_TYPES} value={form.employment_type} onChange={(e) => setField("employment_type", e.target.value)} />
              </FormField>
            </div>
            <div className="grid grid-cols-3 gap-4">
              <FormField label="メール">
                <FormInput type="email" value={form.email} onChange={(e) => setField("email", e.target.value)} />
              </FormField>
              <FormField label="入社日">
                <FormInput type="date" value={form.hire_date} onChange={(e) => setField("hire_date", e.target.value)} />
              </FormField>
              <FormField label="生年月日">
                <FormInput type="date" value={form.birth_date} onChange={(e) => setField("birth_date", e.target.value)} />
              </FormField>
            </div>
            <div className="grid grid-cols-4 gap-4">
              <FormField label="基本給">
                <FormInput type="number" value={form.base_salary} onChange={(e) => setField("base_salary", e.target.value)} />
              </FormField>
              <FormField label="役職手当">
                <FormInput type="number" value={form.position_allowance} onChange={(e) => setField("position_allowance", e.target.value)} />
              </FormField>
              <FormField label="住宅手当">
                <FormInput type="number" value={form.housing_allowance} onChange={(e) => setField("housing_allowance", e.target.value)} />
              </FormField>
              <FormField label="通勤手当">
                <FormInput type="number" value={form.commuting_allowance} onChange={(e) => setField("commuting_allowance", e.target.value)} />
              </FormField>
            </div>
            <div className="grid grid-cols-3 gap-4">
              <FormField label="標準月額報酬">
                <FormInput type="number" value={form.standard_remuneration} onChange={(e) => setField("standard_remuneration", e.target.value)} />
              </FormField>
              <FormField label="標準月間時間">
                <FormInput type="number" step="0.5" value={form.standard_monthly_hours} onChange={(e) => setField("standard_monthly_hours", e.target.value)} />
              </FormField>
              <FormField label="扶養人数">
                <FormInput type="number" min="0" value={form.dependents_count} onChange={(e) => setField("dependents_count", e.target.value)} />
              </FormField>
            </div>
            {updateMutation.isError && (
              <p className="text-sm text-red-400">エラー: {(updateMutation.error as Error).message}</p>
            )}
          </FormModal>

          {/* 削除確認ダイアログ */}
          <ConfirmDialog
            open={deleteOpen}
            title="社員を削除"
            description={`社員 ${data.last_name} ${data.first_name}（${data.employee_id}）を退職扱いにしてよろしいですか？`}
            confirmLabel="削除する"
            variant="danger"
            loading={deleteMutation.isPending}
            onConfirm={() => deleteMutation.mutate()}
            onCancel={() => setDeleteOpen(false)}
          />
        </>
      )}
    </DetailLayout>
  );
}
