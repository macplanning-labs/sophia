"use client";

import { useState } from "react";
import { FormModal, FormField, FormInput } from "@/components/ui/form-modal";

const ROLE_CODE_PATTERN = /^[a-z][a-z0-9_]{0,49}$/;

export interface RoleFormValues {
  code: string;
  name: string;
}

interface RoleFormModalProps {
  mode: "create" | "rename";
  /** 名前変更のとき、今の表示名とコード */
  initial?: { code: string; name: string };
  loading: boolean;
  onSubmit: (values: RoleFormValues) => void;
  onClose: () => void;
}

/** ロールの追加（コードと名前）と、表示名の変更（コードは変えられない）。 */
export function RoleFormModal({ mode, initial, loading, onSubmit, onClose }: RoleFormModalProps) {
  const [code, setCode] = useState(initial?.code ?? "");
  const [name, setName] = useState(initial?.name ?? "");
  const [errors, setErrors] = useState<{ code?: string; name?: string }>({});

  const submit = () => {
    const next: { code?: string; name?: string } = {};
    if (mode === "create" && !ROLE_CODE_PATTERN.test(code.trim())) {
      next.code = "英小文字で始まり、英小文字・数字・_ だけで、50文字以内にしてください";
    }
    if (!name.trim()) next.name = "ロール名を入力してください";
    setErrors(next);
    if (next.code || next.name) return;
    onSubmit({ code: code.trim(), name: name.trim() });
  };

  return (
    <FormModal
      open
      title={mode === "create" ? "ロールを追加" : "ロール名を変更"}
      loading={loading}
      onSubmit={submit}
      onClose={onClose}
    >
      <FormField label="ロールコード" required={mode === "create"} error={errors.code}>
        <FormInput
          value={code}
          onChange={(e) => setCode(e.target.value)}
          disabled={mode === "rename"}
          placeholder="例: sales_lead"
        />
        <p className="text-[11px] text-muted-foreground mt-1">
          {mode === "create"
            ? "作成後は変えられません。英小文字で始めてください。"
            : "コードは変更できません。"}
        </p>
      </FormField>
      <FormField label="ロール名" required error={errors.name}>
        <FormInput value={name} onChange={(e) => setName(e.target.value)} placeholder="例: 営業リーダー" />
      </FormField>
    </FormModal>
  );
}
