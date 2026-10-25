"use client";

import { useState } from "react";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { buildOrgTree, descendantIds, type OrgUnit } from "@/lib/org-admin";

export interface OrgFormValues {
  name: string;
  unit_kind: string | null;
  sort_order: number;
}

interface OrgFormModalProps {
  /** 新規のときは親の組織名、編集のときは対象の組織 */
  mode: "create" | "edit";
  parentName?: string;
  initial?: OrgUnit;
  loading: boolean;
  onSubmit: (values: OrgFormValues) => void;
  onClose: () => void;
}

/** 組織の追加・名前/種別/表示順の編集。呼び出し側は key を付けて開くたびに初期化する。 */
export function OrgFormModal({ mode, parentName, initial, loading, onSubmit, onClose }: OrgFormModalProps) {
  const [name, setName] = useState(initial?.name ?? "");
  const [kind, setKind] = useState(initial?.unit_kind ?? "");
  const [sortOrder, setSortOrder] = useState(String(initial?.sort_order ?? 0));
  const [error, setError] = useState<string | undefined>(undefined);

  const submit = () => {
    if (!name.trim()) {
      setError("組織名を入力してください");
      return;
    }
    const order = Number(sortOrder);
    if (!Number.isInteger(order)) {
      setError("表示順は整数で入力してください");
      return;
    }
    setError(undefined);
    onSubmit({ name: name.trim(), unit_kind: kind.trim() || null, sort_order: order });
  };

  return (
    <FormModal
      open
      title={mode === "create" ? "組織を追加" : "組織を編集"}
      loading={loading}
      onSubmit={submit}
      onClose={onClose}
    >
      {mode === "create" && (
        <FormField label="親の組織">
          <FormInput value={parentName ?? ""} disabled readOnly />
        </FormField>
      )}
      <FormField label="組織名" required error={error}>
        <FormInput value={name} onChange={(e) => setName(e.target.value)} placeholder="例: 営業一課" />
      </FormField>
      <FormField label="組織の種別（任意）">
        <FormInput value={kind} onChange={(e) => setKind(e.target.value)} placeholder="例: 課、部、本部" />
      </FormField>
      <FormField label="表示順">
        <FormInput type="number" value={sortOrder} onChange={(e) => setSortOrder(e.target.value)} />
        <p className="text-[11px] text-muted-foreground mt-1">小さい数字から先に表示されます。同じ数字なら追加された順です。</p>
      </FormField>
    </FormModal>
  );
}

interface MoveOrgModalProps {
  target: OrgUnit;
  units: OrgUnit[];
  loading: boolean;
  onSubmit: (newParentId: number) => void;
  onClose: () => void;
}

/** 組織の付け替え先を選ぶ。自分と配下、無効な組織、今の親は候補から外す。 */
export function MoveOrgModal({ target, units, loading, onSubmit, onClose }: MoveOrgModalProps) {
  const [parentId, setParentId] = useState("");
  const [error, setError] = useState<string | undefined>(undefined);
  const excluded = descendantIds(units, target.id);
  const candidates = buildOrgTree(units).filter(
    (u) => u.is_active && !excluded.has(u.id) && u.id !== target.parent_id,
  );

  const submit = () => {
    if (!parentId) {
      setError("付け替え先の組織を選んでください");
      return;
    }
    setError(undefined);
    onSubmit(Number(parentId));
  };

  return (
    <FormModal open title="組織の付け替え" loading={loading} onSubmit={submit} onClose={onClose}>
      <FormField label="現在の組織">
        <FormInput value={target.name} disabled readOnly />
      </FormField>
      <FormField label="付け替え先（新しい親）" required error={error}>
        <FormSelect
          value={parentId}
          onChange={(e) => setParentId(e.target.value)}
          placeholder="選択..."
          options={candidates.map((u) => ({
            value: String(u.id),
            label: `${"　".repeat(u.depth)}${u.name}`,
          }))}
        />
      </FormField>
      <p className="text-[11px] text-muted-foreground">
        付け替えると、この組織の配下もまとめて移動します。ルート組織は動かせません。
      </p>
    </FormModal>
  );
}
