"use client";

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FormModal } from "@/components/ui/form-modal";
import { OrgEmployeePicker } from "@/components/org-admin/org-employee-picker";
import {
  buildHandOverSummary,
  fetchOwnedByEmployee,
  type OwnedCount,
  type TransferTo,
} from "@/lib/owner-transfer";

interface HandOverDialogProps {
  /** 画面に出す対象（例: 「社員 山田 太郎」） */
  subjectLabel: string;
  /** 409 で返ってきた担当データの件数 */
  owned: OwnedCount[];
  /** 対象の社員ID。分かるときは組織ごとの内訳を取る。分からなければ件数だけ出す */
  employeeId: number | null;
  submitLabel: string;
  loading: boolean;
  onSubmit: (to: TransferTo) => void;
  onClose: () => void;
}

/**
 * 担当データのある社員(またはその社員に結びつくユーザー)を無効にするときの引き継ぎ先の選択。
 * 409 を受けたときに開く。選んだ組織と担当者へ、担当データがすべて移る。
 */
export function HandOverDialog({ subjectLabel, owned, employeeId, submitLabel, loading, onSubmit, onClose }: HandOverDialogProps) {
  const [orgId, setOrgId] = useState("");
  const [toEmployeeId, setToEmployeeId] = useState("");
  const [error, setError] = useState<string | undefined>(undefined);

  const { data: byEmployee } = useQuery({
    queryKey: ["owner-transfer", "by-employee", employeeId],
    queryFn: () => fetchOwnedByEmployee(employeeId as number),
    enabled: employeeId !== null,
  });

  const summary = buildHandOverSummary(owned, byEmployee?.by_org ?? []);

  const submit = () => {
    if (!orgId || !toEmployeeId) {
      setError("引き継ぎ先の組織と担当者を選んでください");
      return;
    }
    setError(undefined);
    onSubmit({ org_unit_id: Number(orgId), employee_id: Number(toEmployeeId) });
  };

  return (
    <FormModal
      open
      title="担当の引き継ぎ"
      size="sm"
      submitLabel={submitLabel}
      loading={loading}
      onSubmit={submit}
      onClose={onClose}
    >
      <div className="space-y-1">
        <p className="text-xs text-muted-foreground">{subjectLabel} が担当しているデータがあります。引き継ぎ先を選んでください。</p>
      </div>

      <div className="rounded-lg border border-border bg-background/60 p-3 space-y-2">
        <p className="text-xs font-medium text-muted-foreground">担当しているデータ（合計 {summary.total}件）</p>
        <ul className="text-sm space-y-1">
          {summary.rows.map((r) => (
            <li key={r.table} className="flex justify-between gap-4">
              <span>{r.label}</span>
              <span className="tabular-nums">{r.count}件</span>
            </li>
          ))}
        </ul>
        {summary.orgLines.length > 0 && (
          <div className="border-t border-border pt-2 space-y-1">
            <p className="text-xs font-medium text-muted-foreground">組織ごと</p>
            <ul className="text-sm space-y-1">
              {summary.orgLines.map((o) => (
                <li key={o.label} className="flex justify-between gap-4">
                  <span>{o.label}</span>
                  <span className="tabular-nums">{o.count}件</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <OrgEmployeePicker
        orgId={orgId}
        employeeId={toEmployeeId}
        onOrgChange={setOrgId}
        onEmployeeChange={setToEmployeeId}
        excludeEmployeeId={employeeId}
        error={error}
      />

      <div className="rounded-lg border border-amber-500/40 bg-amber-500/10 p-3 text-sm text-amber-300">
        担当データは、すべて選んだ組織に移ります。
      </div>
    </FormModal>
  );
}
