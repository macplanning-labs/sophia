"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { fetchMasterDetail, fetchMastersMeta, updateMasterRecord } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Button } from "@/components/ui/button";
import { FormModal } from "@/components/ui/form-modal";
import { Pencil, UserPlus } from "lucide-react";
import { AddMemberModal } from "@/components/members/add-member-modal";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { OwnerPanel } from "@/components/org-admin/owner-panel";
import { DynamicField } from "@/components/masters/dynamic-field";
import { partiesHref } from "@/lib/parties-utils";
import {
  accountNumberColumnError,
  groupPartyColumns,
  missingRequired,
  optionLabel,
  toMasterFormData,
  toMasterPayload,
  type FormSection,
  type MasterFormColumnMeta,
  type MasterFormData,
  type MasterTableMeta,
} from "@/lib/master-form";

type PartyType = "client" | "partner";

interface Props {
  id?: string;
  type?: PartyType;
  embedded?: boolean;
  /** 開いたらそのまま編集できる表示にする(一覧の行から開いたとき)。管理者だけ。 */
  editInline?: boolean;
  /** 編集を保存したあとに呼ぶ(一覧のモーダルを閉じるなど) */
  onSaved?: () => void;
}

/**
 * 取引先(クライアント・パートナー)の詳細・編集。
 * 項目はマスタ定義(/api/v1/masters/meta)から作るので、マスタメンテにあった項目をすべて編集できる。
 */
export default function PartyDetailPage({ id: idProp, type: typeProp, embedded = false, editInline = false, onSaved }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const type = typeProp || "client";
  const qc = useQueryClient();
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const mounted = useMounted();
  const showAdmin = isAdmin && mounted;

  const tableKey = type === "client" ? "clients" : "partners";
  const { data, isLoading } = useQuery<Record<string, unknown>>({
    queryKey: ["masters-data", tableKey, id],
    queryFn: () => fetchMasterDetail(tableKey, id),
    enabled: !!id,
  });
  const { data: tables = [], isLoading: metaLoading } = useQuery<MasterTableMeta[]>({
    queryKey: ["masters-meta"],
    queryFn: fetchMastersMeta,
    staleTime: 5 * 60 * 1000,
  });
  const cols: MasterFormColumnMeta[] = tables.find((t) => t.key === tableKey)?.form_columns ?? [];
  const sections = groupPartyColumns(type, cols);

  const [editOpen, setEditOpen] = useState(false);
  const [addMemberOpen, setAddMemberOpen] = useState(false);
  const [form, setForm] = useState<MasterFormData>({});
  const inline = editInline && showAdmin;

  const updateMutation = useMutation({
    mutationFn: (payload: Record<string, unknown>) => updateMasterRecord(tableKey, id, payload),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["masters-data", tableKey, id] });
      qc.invalidateQueries({ queryKey: ["masters-data", tableKey] });
      setEditOpen(false);
      toast.success("更新しました");
      onSaved?.();
    },
    onError: (e: Error) => toast.error(`更新に失敗しました: ${e.message}`),
  });

  const handleEditClick = () => {
    if (data) {
      setForm(toMasterFormData(cols, data));
      setEditOpen(true);
    }
  };

  const save = (fd: MasterFormData) => {
    const missing = missingRequired(cols, fd);
    if (missing.length > 0) {
      toast.error(`${missing.join("・")}は必須です`);
      return;
    }
    const accountError = accountNumberColumnError(cols, fd);
    if (accountError) {
      toast.error(accountError);
      return;
    }
    updateMutation.mutate(toMasterPayload(cols, fd));
  };

  return (
    <DetailLayout
      title={`${type === "client" ? "クライアント" : "パートナー"} #${id}`}
      icon="🏢"
      backHref={partiesHref(type)}
      backLabel="一覧に戻る"
      isLoading={isLoading || metaLoading}
      embedded={embedded}
    >
      {data && (
        <>
          {/* Action bar */}
          {showAdmin && !userLoading && (!inline || type === "partner") && (
            <div className="flex items-center gap-2 mb-4 p-3 bg-card border border-border rounded-lg">
              {!inline && (
                <Button
                  variant="outline"
                  size="sm"
                  className="border-border text-foreground gap-1"
                  onClick={handleEditClick}
                >
                  <Pencil className="w-3.5 h-3.5" /> 編集
                </Button>
              )}
              {type === "partner" && (
                <Button
                  variant="outline"
                  size="sm"
                  className="border-border text-foreground gap-1"
                  onClick={() => setAddMemberOpen(true)}
                >
                  <UserPlus className="w-3.5 h-3.5" /> 要員追加
                </Button>
              )}
            </div>
          )}

          {/* Owner (Admin only) */}
          {type === "client" && <div className="mb-4"><OwnerPanel target={{ client_id: Number(id) }} subject="顧客" /></div>}

          {inline ? (
            <InlinePartyEditor
              key={String(id)}
              initial={toMasterFormData(cols, data)}
              sections={sections}
              saving={updateMutation.isPending}
              onSave={save}
            />
          ) : (
            <PartyReadOnly sections={sections} data={data} />
          )}

          {/* Edit Modal */}
          {showAdmin && (
            <FormModal
              open={editOpen}
              title={`${type === "client" ? "クライアント" : "パートナー"}編集`}
              size="lg"
              loading={updateMutation.isPending}
              submitLabel="保存"
              onSubmit={() => save(form)}
              onClose={() => setEditOpen(false)}
            >
              <PartyFormSections sections={sections} form={form} setForm={setForm} />
            </FormModal>
          )}

          {type === "partner" && addMemberOpen && (
            <AddMemberModal
              open
              onClose={() => setAddMemberOpen(false)}
              classification="PARTNER"
              defaultPartnerId={id}
              lockPartner
            />
          )}
        </>
      )}
    </DetailLayout>
  );
}

/** 区切りごとの入力欄(マスタ定義の全項目)。 */
function PartyFormSections({
  sections,
  form,
  setForm,
}: {
  sections: FormSection[];
  form: MasterFormData;
  setForm: (f: MasterFormData) => void;
}) {
  return (
    <div className="space-y-6">
      {sections.map((s) => (
        <div key={s.title} className="space-y-4">
          <h3 className="font-semibold text-sm border-b border-border pb-2">{s.title}</h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {s.cols.map((col) => (
              <DynamicField
                key={col.name}
                col={col}
                value={form[col.name] ?? ""}
                onChange={(v) => setForm({ ...form, [col.name]: v })}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/** 一覧の行から開いたときの編集フォーム。開いた時点の内容で始め、保存ボタンで更新する。 */
function InlinePartyEditor({
  initial,
  sections,
  saving,
  onSave,
}: {
  initial: MasterFormData;
  sections: FormSection[];
  saving: boolean;
  onSave: (f: MasterFormData) => void;
}) {
  const [form, setForm] = useState<MasterFormData>(initial);
  return (
    <div className="bg-card border border-border rounded-lg p-4 space-y-4">
      <PartyFormSections sections={sections} form={form} setForm={setForm} />
      <div className="flex justify-end">
        <Button size="sm" onClick={() => onSave(form)} disabled={saving}>
          {saving ? "保存中..." : "保存"}
        </Button>
      </div>
    </div>
  );
}

/** 見るだけの表示(区切りごと)。 */
function PartyReadOnly({ sections, data }: { sections: FormSection[]; data: Record<string, unknown> }) {
  const show = (col: MasterFormColumnMeta) => {
    const v = data[col.name];
    if (col.input_type === "checkbox") return v === true ? "はい" : "いいえ";
    if (col.input_type === "select") return optionLabel(col, v);
    return v === null || v === undefined || v === "" ? "—" : String(v);
  };
  return (
    <div className="bg-card border border-border rounded-lg p-4 space-y-4">
      {sections.map((s) => (
        <div key={s.title} className="space-y-2">
          <h3 className="font-semibold text-sm">{s.title}</h3>
          <FieldGrid>
            {s.cols.map((col) => (
              <Field key={col.name} label={col.label} value={show(col)} />
            ))}
          </FieldGrid>
        </div>
      ))}
    </div>
  );
}
