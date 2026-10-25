"use client";

import { useState } from "react";
import { FormModal, FormField, FormSelect } from "@/components/ui/form-modal";
import { buildOrgTree, disabledOrgIds, type OrgUnit, type RoleRow } from "@/lib/org-admin";
import type { UserRow } from "@/lib/types";

export interface AssignmentFormValues {
  user_id: number;
  org_unit_id: number;
  role_id: number;
}

interface AssignmentFormModalProps {
  users: UserRow[];
  /** 有効な組織だけを渡すこと */
  units: OrgUnit[];
  roles: RoleRow[];
  loading: boolean;
  onSubmit: (values: AssignmentFormValues) => void;
  onClose: () => void;
}

/** ユーザー・組織・ロールを選んで割り当てを追加する。 */
export function AssignmentFormModal({ users, units, roles, loading, onSubmit, onClose }: AssignmentFormModalProps) {
  const [userId, setUserId] = useState("");
  const [orgId, setOrgId] = useState("");
  const [roleId, setRoleId] = useState("");
  const [error, setError] = useState<string | undefined>(undefined);

  // 管理者は常に全権限を持つため、割り当ての対象から外す。
  const activeUsers = users.filter((u) => u.is_active && !u.is_staff);
  // 無効な組織とその配下には割り当てない(割り当てても権限として働かない)。木は全組織で作ってから外す。
  const disabled = disabledOrgIds(units);
  const orgOptions = buildOrgTree(units).filter((n) => !disabled.has(n.id));

  const submit = () => {
    if (!userId || !orgId || !roleId) {
      setError("ユーザー・組織・ロールをすべて選んでください");
      return;
    }
    setError(undefined);
    onSubmit({ user_id: Number(userId), org_unit_id: Number(orgId), role_id: Number(roleId) });
  };

  return (
    <FormModal open title="割り当てを追加" loading={loading} onSubmit={submit} onClose={onClose}>
      <FormField label="ユーザー" required error={error}>
        <FormSelect
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          placeholder="選択..."
          options={activeUsers.map((u) => ({ value: String(u.id), label: `${u.username}（${u.email}）` }))}
        />
      </FormField>
      <FormField label="組織" required>
        <FormSelect
          value={orgId}
          onChange={(e) => setOrgId(e.target.value)}
          placeholder="選択..."
          options={orgOptions.map((u) => ({ value: String(u.id), label: `${"　".repeat(u.depth)}${u.name}` }))}
        />
      </FormField>
      <FormField label="ロール" required>
        <FormSelect
          value={roleId}
          onChange={(e) => setRoleId(e.target.value)}
          placeholder="選択..."
          options={roles.map((r) => ({ value: String(r.id), label: r.name }))}
        />
      </FormField>
      <p className="text-[11px] text-muted-foreground">
        無効なユーザーと無効な組織には割り当てられません。全社の権限を含むロールは、ルート組織にだけ割り当てられます。
      </p>
    </FormModal>
  );
}
