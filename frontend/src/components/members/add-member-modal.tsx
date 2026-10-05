"use client";

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { fetchMastersData, fetchMembers, createMasterRecord, createMember, updateMasterRecord } from "@/lib/api";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import {
  validateAddMember,
  buildMemberPayload,
  buildPartnerPayload,
  buildAttachPayload,
  attachCandidates,
  type AddMemberForm,
  type AddMode,
  type Classification,
} from "@/lib/add-member";

interface PartnerOption {
  partner_id: string;
  name: string;
}

interface Props {
  open: boolean;
  onClose: () => void;
  /** 入口で決まる区分。自社社員追加 / パートナー要員追加(画面内では変えられない) */
  classification: Classification;
  /** 開いたときに選んでおくパートナー(パートナーの詳細から開くとき) */
  defaultPartnerId?: string;
  /** true なら、パートナーを変えられない(パートナーの詳細から開くとき) */
  lockPartner?: boolean;
  onCreated?: () => void;
}

const emptyNewPartner = { name: "", contact_person: "", email: "" };

/**
 * 要員追加。入口(自社社員追加 / パートナー要員追加)で区分が決まる。
 * パートナー要員は、所属のパートナーを選び(未登録ならその場で追加)、
 * 「新規の要員を追加」か「登録済みの要員を選ぶ」かを選ぶ。
 */
export function AddMemberModal({ open, onClose, classification, defaultPartnerId = "", lockPartner = false, onCreated }: Props) {
  const queryClient = useQueryClient();
  const isPartner = classification === "PARTNER";
  const [form, setForm] = useState<AddMemberForm>({
    classification,
    mode: "new",
    name: "",
    name_kana: "",
    email: "",
    partner_id: defaultPartnerId,
    newPartner: null,
    existing_id: "",
  });

  const { data: partners = [] } = useQuery<PartnerOption[]>({
    queryKey: ["masters-data", "partners"],
    queryFn: () => fetchMastersData("partners"),
    enabled: open && isPartner,
  });
  const { data: members = [] } = useQuery({
    queryKey: ["members"],
    queryFn: fetchMembers,
    enabled: open && isPartner,
  });

  const partnerNameById = useMemo(
    () => Object.fromEntries(partners.map((p) => [p.partner_id, p.name])),
    [partners],
  );
  const candidates = useMemo(
    () => attachCandidates(members, form.partner_id, partnerNameById),
    [members, form.partner_id, partnerNameById],
  );
  const selectedCandidate = candidates.find((c) => c.value === form.existing_id);

  const mutation = useMutation({
    mutationFn: async () => {
      const problem = validateAddMember(form);
      if (problem) throw new Error(problem);

      let partnerId: string | null = form.partner_id || null;
      if (isPartner && form.newPartner) {
        // 先にパートナーを作り、作ったIDで要員を作る/所属させる
        const res = await createMasterRecord("partners", buildPartnerPayload(form.newPartner));
        if (res?.ok === false) throw new Error(res.error || "パートナーの追加に失敗しました");
        if (!res?.id) throw new Error("パートナーを追加しましたが、IDを取得できませんでした。パートナー一覧から選んで要員を追加してください");
        partnerId = String(res.id);
        queryClient.invalidateQueries({ queryKey: ["masters-data", "partners"] });
      }

      if (isPartner && form.mode === "existing") {
        const member = members.find((m) => String(m.id) === form.existing_id);
        if (!member) throw new Error("選んだ要員が見つかりません。画面を更新してください");
        const res = await updateMasterRecord("engineers", String(member.id), buildAttachPayload(member, partnerId!));
        if (res?.ok === false) throw new Error(res.error || "所属パートナーの変更に失敗しました");
        return;
      }
      await createMember(buildMemberPayload(form, partnerId));
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["members"] });
      toast.success(form.mode === "existing" && isPartner ? "要員を所属させました" : "要員を追加しました");
      onCreated?.();
      onClose();
    },
    onError: (err: Error) => {
      toast.error(err.message || "要員の追加に失敗しました");
    },
  });

  const set = <K extends keyof AddMemberForm>(key: K, value: AddMemberForm[K]) =>
    setForm((f) => ({ ...f, [key]: value }));
  const setMode = (mode: AddMode) => setForm((f) => ({ ...f, mode, existing_id: "" }));

  if (!open) return null;

  return (
    <FormModal
      open
      title={isPartner ? "パートナー要員追加" : "自社社員追加"}
      submitLabel="追加"
      loading={mutation.isPending}
      onClose={onClose}
      onSubmit={() => mutation.mutate()}
    >
      <div className="space-y-4">
        {isPartner && (
          <div className="space-y-3">
            {!form.newPartner ? (
              <FormField label="パートナー" required>
                <div className="space-y-2">
                  <FormSelect
                    value={form.partner_id}
                    disabled={lockPartner}
                    onChange={(e) => setForm((f) => ({ ...f, partner_id: e.target.value, existing_id: "" }))}
                    placeholder="パートナーを選択"
                    options={partners.map((p) => ({ value: p.partner_id, label: p.name }))}
                  />
                  {!lockPartner && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => setForm((f) => ({ ...f, partner_id: "", existing_id: "", newPartner: { ...emptyNewPartner } }))}
                    >
                      パートナー追加
                    </Button>
                  )}
                </div>
              </FormField>
            ) : (
              <div className="rounded-md border border-border p-3 space-y-3">
                <div className="flex items-center justify-between">
                  <p className="text-sm font-medium">新しいパートナー</p>
                  <Button type="button" variant="ghost" size="sm" onClick={() => set("newPartner", null)}>
                    既存から選択
                  </Button>
                </div>
                <FormField label="名称" required>
                  <FormInput
                    value={form.newPartner.name}
                    onChange={(e) => set("newPartner", { ...form.newPartner!, name: e.target.value })}
                    placeholder="株式会社○○"
                  />
                </FormField>
                <FormField label="担当者">
                  <FormInput
                    value={form.newPartner.contact_person}
                    onChange={(e) => set("newPartner", { ...form.newPartner!, contact_person: e.target.value })}
                  />
                </FormField>
                <FormField label="メール">
                  <FormInput
                    type="email"
                    value={form.newPartner.email}
                    onChange={(e) => set("newPartner", { ...form.newPartner!, email: e.target.value })}
                  />
                </FormField>
              </div>
            )}

            {/* 新規の要員 / 登録済みの要員 */}
            <div className="flex gap-2" role="group" aria-label="追加方法">
              <Button type="button" size="sm" variant={form.mode === "new" ? "default" : "outline"} onClick={() => setMode("new")}>
                新規の要員
              </Button>
              <Button type="button" size="sm" variant={form.mode === "existing" ? "default" : "outline"} onClick={() => setMode("existing")}>
                登録済みの要員
              </Button>
            </div>
          </div>
        )}

        {isPartner && form.mode === "existing" ? (
          <FormField label="要員" required>
            <div className="space-y-2">
              <FormSelect
                value={form.existing_id}
                onChange={(e) => set("existing_id", e.target.value)}
                placeholder={candidates.length ? "登録済みの要員を選択" : "選べる要員がいません"}
                options={candidates.map((c) => ({ value: c.value, label: c.label }))}
              />
              {selectedCandidate?.currentPartnerName && (
                <p className="text-xs text-amber-400">
                  この要員の所属を「{selectedCandidate.currentPartnerName}」から、選んだパートナーに変更します。
                </p>
              )}
            </div>
          </FormField>
        ) : (
          <>
            <FormField label="氏名" required>
              <FormInput value={form.name} onChange={(e) => set("name", e.target.value)} placeholder="山田 太郎" />
            </FormField>
            <FormField label="カナ">
              <FormInput value={form.name_kana} onChange={(e) => set("name_kana", e.target.value)} placeholder="ヤマダ タロウ" />
            </FormField>
            <FormField label="メール">
              <FormInput type="email" value={form.email} onChange={(e) => set("email", e.target.value)} />
            </FormField>
          </>
        )}
      </div>
    </FormModal>
  );
}
