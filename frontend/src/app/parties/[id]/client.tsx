"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { fetchMasterDetail, updateMasterRecord } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Button } from "@/components/ui/button";
import { FormModal } from "@/components/ui/form-modal";
import { Pencil, UserPlus } from "lucide-react";
import { AddMemberModal } from "@/components/members/add-member-modal";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { formatBillingUnit } from "@/lib/parties-utils";

type PartyType = "client" | "partner";

interface ClientData {
  id: number;
  name: string;
  contact_person?: string;
  email?: string;
  billing_unit?: string;
  edi_format?: string;
  [key: string]: unknown;
}

interface PartnerData {
  partner_id: string;
  name: string;
  contact_person?: string;
  email?: string;
  [key: string]: unknown;
}

type PartyData = ClientData | PartnerData;

interface Props {
  id?: string;
  type?: PartyType;
  embedded?: boolean;
}

export default function PartyDetailPage({ id: idProp, type: typeProp, embedded = false }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const type = typeProp || "client";
  const qc = useQueryClient();
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const mounted = useMounted();
  const showAdmin = isAdmin && mounted;

  const tableKey = type === "client" ? "clients" : "partners";
  const { data, isLoading } = useQuery({
    queryKey: ["masters-data", tableKey, id],
    queryFn: () => fetchMasterDetail(tableKey, id),
    enabled: !!id,
  });

  // Edit modal
  const [editOpen, setEditOpen] = useState(false);
  const [addMemberOpen, setAddMemberOpen] = useState(false);
  const [form, setForm] = useState({
    name: "",
    contact_person: "",
    email: "",
    billing_unit: "PROJECT",
    edi_format: "",
  });

  const updateMutation = useMutation({
    mutationFn: (payload: Record<string, unknown>) =>
      updateMasterRecord(tableKey, id, payload),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["masters-data", tableKey, id] });
      qc.invalidateQueries({ queryKey: ["masters-data", tableKey] });
      setEditOpen(false);
      toast.success("更新しました");
    },
    onError: (e: Error) => toast.error(`更新に失敗しました: ${e.message}`),
  });

  const handleEditClick = () => {
    if (data) {
      setForm({
        name: (data as PartyData).name || "",
        contact_person: (data as PartyData).contact_person || "",
        email: (data as PartyData).email || "",
        billing_unit: type === "client" ? ((data as ClientData).billing_unit || "PROJECT") : "PROJECT",
        edi_format: type === "client" ? ((data as ClientData).edi_format || "") : "",
      });
      setEditOpen(true);
    }
  };

  const handleSave = async () => {
    if (!form.name.trim()) {
      toast.error("名称は必須です");
      return;
    }

    const payload: Record<string, unknown> = {
      name: form.name,
      contact_person: form.contact_person || null,
      email: form.email || null,
    };

    if (type === "client") {
      payload.billing_unit = form.billing_unit;
      payload.edi_format = form.edi_format || null;
    }

    updateMutation.mutate(payload);
  };

  return (
    <DetailLayout
      title={`${type === "client" ? "クライアント" : "パートナー"} #${id}`}
      icon="🏢"
      backHref="/parties"
      backLabel="一覧に戻る"
      isLoading={isLoading}
      embedded={embedded}
    >
      {data && (
        <>
          {/* Action bar */}
          {showAdmin && !userLoading && (
            <div className="flex items-center gap-2 mb-4 p-3 bg-card border border-border rounded-lg">
              <Button
                variant="outline"
                size="sm"
                className="border-border text-foreground gap-1"
                onClick={handleEditClick}
              >
                <Pencil className="w-3.5 h-3.5" /> 編集
              </Button>
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

          {/* Basic info */}
          <div className="bg-card border border-border rounded-lg p-4 space-y-4">
            <h3 className="font-semibold text-sm">基本情報</h3>
            <FieldGrid>
              <Field label="名称" value={(data as PartyData).name} />
              <Field label="担当者" value={(data as PartyData).contact_person || "—"} />
              <Field label="メール" value={(data as PartyData).email || "—"} />
            </FieldGrid>

            {type === "client" && (
              <>
                <div className="border-t border-border pt-4 space-y-4">
                  <h3 className="font-semibold text-sm">請求設定</h3>
                  <FieldGrid>
                    <Field
                      label="請求単位"
                      value={formatBillingUnit((data as ClientData).billing_unit)}
                    />
                    <Field
                      label="EDI方式"
                      value={(data as ClientData).edi_format || "—"}
                    />
                  </FieldGrid>
                </div>
              </>
            )}
          </div>

          {/* Edit Modal */}
          {showAdmin && (
            <FormModal
              open={editOpen}
              title={`${type === "client" ? "クライアント" : "パートナー"}編集`}
              size="lg"
              loading={updateMutation.isPending}
              submitLabel="保存"
              onSubmit={handleSave}
              onClose={() => setEditOpen(false)}
            >
              <div className="grid grid-cols-1 gap-4">
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">名称 *</label>
                  <input
                    className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                    value={form.name}
                    onChange={(e) => setForm({ ...form, name: e.target.value })}
                    placeholder="例: 株式会社サンプル"
                  />
                </div>
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">担当者</label>
                  <input
                    className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                    value={form.contact_person}
                    onChange={(e) => setForm({ ...form, contact_person: e.target.value })}
                    placeholder="例: 山田太郎"
                  />
                </div>
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">メール</label>
                  <input
                    type="email"
                    className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                    value={form.email}
                    onChange={(e) => setForm({ ...form, email: e.target.value })}
                    placeholder="例: sample@example.com"
                  />
                </div>
                {type === "client" && (
                  <>
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">請求単位</label>
                      <select
                        className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                        value={form.billing_unit}
                        onChange={(e) => setForm({ ...form, billing_unit: e.target.value })}
                      >
                        <option value="PROJECT">案件ごと</option>
                        <option value="CLIENT">取引先まとめ</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">EDI方式</label>
                      <input
                        className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                        value={form.edi_format}
                        onChange={(e) => setForm({ ...form, edi_format: e.target.value })}
                        placeholder="例: 取引先のEDIシステム名"
                      />
                    </div>
                  </>
                )}
              </div>
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
