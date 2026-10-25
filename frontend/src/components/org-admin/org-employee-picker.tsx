"use client";

import { useQuery } from "@tanstack/react-query";
import { FormField, FormSelect } from "@/components/ui/form-modal";
import { buildOrgTree, disabledOrgIds, fetchOrgUnits } from "@/lib/org-admin";
import { fetchEmployees } from "@/lib/api";
import { EMPLOYEE_LIST_QUERY_KEY } from "@/lib/owner-transfer";

interface OrgEmployeePickerProps {
  orgId: string;
  employeeId: string;
  onOrgChange: (value: string) => void;
  onEmployeeChange: (value: string) => void;
  /** 退職する社員など、担当者の候補から外す社員ID */
  excludeEmployeeId?: number | null;
  /** 組織・担当者の必須表示と、エラー表示 */
  error?: string;
}

/**
 * 担当の組織（有効なもの。無効な組織の配下は除く）と担当者（有効な社員のみ）を選ぶ。
 * 担当の変更・退職時の引き継ぎ・自動取込の受け皿で共通に使う。
 */
export function OrgEmployeePicker({ orgId, employeeId, onOrgChange, onEmployeeChange, excludeEmployeeId, error }: OrgEmployeePickerProps) {
  const { data: units } = useQuery({ queryKey: ["org-units"], queryFn: fetchOrgUnits });
  const { data: employees } = useQuery({ queryKey: EMPLOYEE_LIST_QUERY_KEY, queryFn: fetchEmployees });

  const disabled = units ? disabledOrgIds(units) : new Set<number>();
  const orgOptions = units
    ? buildOrgTree(units)
        .filter((n) => n.is_active && !disabled.has(n.id))
        .map((n) => ({ value: String(n.id), label: `${"　".repeat(n.depth)}${n.name}` }))
    : [];
  const employeeOptions = (employees ?? [])
    .filter((e) => e.is_active && e.id !== excludeEmployeeId)
    .map((e) => ({ value: String(e.id), label: `${e.last_name} ${e.first_name}（${e.employee_id}）` }));

  return (
    <>
      <FormField label="組織" required error={error}>
        <FormSelect value={orgId} onChange={(e) => onOrgChange(e.target.value)} placeholder="選択..." options={orgOptions} />
      </FormField>
      <FormField label="担当者" required>
        <FormSelect value={employeeId} onChange={(e) => onEmployeeChange(e.target.value)} placeholder="選択..." options={employeeOptions} />
      </FormField>
    </>
  );
}
