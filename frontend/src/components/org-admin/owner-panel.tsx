"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Move } from "lucide-react";
import { Button } from "@/components/ui/button";
import { FormModal } from "@/components/ui/form-modal";
import { Field, FieldGrid } from "@/components/detail-layout";
import { OrgEmployeePicker } from "@/components/org-admin/org-employee-picker";
import { errorMessage } from "@/components/org-admin/org-admin-shell";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { fetchOwnerCurrent, refreshAfterOwnerChange, transferOwner, type OwnerTarget } from "@/lib/owner-transfer";

interface OwnerPanelProps {
  target: OwnerTarget;
  /** 画面の対象の呼び名（「顧客」か「案件」）。確認文に使う */
  subject: "顧客" | "案件";
}

/**
 * 顧客・案件の担当（組織・担当者）を表示し、担当を変える。Admin にだけ出す。
 * 担当を変えると、配下の契約・発注・受注・請求書・稼働報告なども同じ担当に変わる。
 */
export function OwnerPanel({ target, subject }: OwnerPanelProps) {
  const qc = useQueryClient();
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const mounted = useMounted();
  const show = isAdmin && mounted && !userLoading;
  const queryKey = ["owner-transfer", "current", target] as const;
  const [open, setOpen] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey,
    queryFn: () => fetchOwnerCurrent(target),
    enabled: show,
  });

  if (!show) return null;

  return (
    <>
      <div className="bg-card border border-border rounded-lg p-4 space-y-3">
        <div className="flex items-center justify-between gap-2">
          <h3 className="font-semibold text-sm">担当</h3>
          <Button
            variant="outline"
            size="sm"
            className="border-border text-foreground gap-1"
            disabled={isLoading || !data}
            onClick={() => setOpen(true)}
          >
            <Move className="w-3.5 h-3.5" /> 担当を変更
          </Button>
        </div>
        {isLoading || !data ? (
          <div className="h-10 bg-muted/50 rounded animate-pulse" />
        ) : (
          <FieldGrid cols={2}>
            <Field label="担当組織" value={data.org_name ?? "未設定"} />
            <Field label="担当者" value={data.employee_name ?? "未設定"} />
          </FieldGrid>
        )}
      </div>

      {open && (
        <OwnerTransferModal
          target={target}
          subject={subject}
          currentOrgId={data?.org_unit_id ?? null}
          currentEmployeeId={data?.employee_id ?? null}
          onClose={() => setOpen(false)}
          onDone={() => {
            // 配下にも広がるので、取引先・案件の一覧なども読み直す
            void refreshAfterOwnerChange(qc);
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

interface OwnerTransferModalProps {
  target: OwnerTarget;
  subject: "顧客" | "案件";
  currentOrgId: number | null;
  currentEmployeeId: number | null;
  onClose: () => void;
  onDone: () => void;
}

function OwnerTransferModal({ target, subject, currentOrgId, currentEmployeeId, onClose, onDone }: OwnerTransferModalProps) {
  const [orgId, setOrgId] = useState(currentOrgId === null ? "" : String(currentOrgId));
  const [employeeId, setEmployeeId] = useState(currentEmployeeId === null ? "" : String(currentEmployeeId));
  const [error, setError] = useState<string | undefined>(undefined);

  const mutation = useMutation({
    mutationFn: () => transferOwner(target, { org_unit_id: Number(orgId), employee_id: Number(employeeId) }),
    onSuccess: (res) => {
      toast.success(`担当を変更しました（${res.total}件）`);
      onDone();
    },
    onError: (e: Error) => toast.error(errorMessage(e, "担当の変更に失敗しました")),
  });

  const submit = () => {
    if (!orgId || !employeeId) {
      setError("組織と担当者をすべて選んでください");
      return;
    }
    setError(undefined);
    mutation.mutate();
  };

  return (
    <FormModal
      open
      title="担当を変更"
      submitLabel="変更する"
      loading={mutation.isPending}
      onSubmit={submit}
      onClose={onClose}
    >
      <OrgEmployeePicker
        orgId={orgId}
        employeeId={employeeId}
        onOrgChange={setOrgId}
        onEmployeeChange={setEmployeeId}
        error={error}
      />
      <p className="text-[11px] text-muted-foreground">
        この{subject}の配下の契約・発注・受注・請求書・稼働報告なども、まとめて同じ担当に変わります。
      </p>
    </FormModal>
  );
}
