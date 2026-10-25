"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Pencil } from "lucide-react";
import { Button } from "@/components/ui/button";
import { FormModal } from "@/components/ui/form-modal";
import { Field, FieldGrid } from "@/components/detail-layout";
import { OrgEmployeePicker } from "@/components/org-admin/org-employee-picker";
import { errorMessage } from "@/components/org-admin/org-admin-shell";
import { fetchOrgUnits } from "@/lib/org-admin";
import { fetchEmployees } from "@/lib/api";
import {
  EMPLOYEE_LIST_QUERY_KEY,
  fetchAutoImportReceiver,
  updateAutoImportReceiver,
} from "@/lib/owner-transfer";

/** 自動取込の受け皿（既定の担当組織と担当者）を表示し、変更する。組織管理画面の下に置く。 */
export function AutoImportReceiverCard() {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);

  const { data: receiver, isLoading } = useQuery({
    queryKey: ["owner-transfer", "receiver"],
    queryFn: fetchAutoImportReceiver,
  });
  const { data: units } = useQuery({ queryKey: ["org-units"], queryFn: fetchOrgUnits });
  const { data: employees } = useQuery({ queryKey: EMPLOYEE_LIST_QUERY_KEY, queryFn: fetchEmployees });

  const orgName = units?.find((u) => u.id === receiver?.org_unit_id)?.name ?? null;
  const emp = employees?.find((e) => e.id === receiver?.employee_id);
  const employeeName = emp ? `${emp.last_name} ${emp.first_name}` : null;

  return (
    <div className="bg-card border border-border rounded-lg p-4 space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-sm font-semibold">自動取込の受け皿</h2>
          <p className="text-xs text-muted-foreground mt-0.5">
            EDI 同期・メール取込で作るデータに、親の担当が無いとき使う担当です。
          </p>
        </div>
        <Button
          size="sm"
          variant="outline"
          className="border-border text-muted-foreground hover:text-foreground"
          disabled={isLoading || !receiver}
          onClick={() => setOpen(true)}
        >
          <Pencil className="w-3.5 h-3.5 mr-1" /> 変更
        </Button>
      </div>

      {isLoading || !receiver ? (
        <div className="h-10 bg-muted/50 rounded animate-pulse" />
      ) : (
        <FieldGrid cols={2}>
          <Field label="担当組織" value={orgName ?? "未設定"} />
          <Field label="担当者" value={employeeName ?? "未設定"} />
        </FieldGrid>
      )}

      {open && receiver && (
        <ReceiverEditModal
          current={{ orgId: receiver.org_unit_id, employeeId: receiver.employee_id }}
          onClose={() => setOpen(false)}
          onSaved={() => {
            qc.invalidateQueries({ queryKey: ["owner-transfer", "receiver"] });
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}

interface ReceiverEditModalProps {
  current: { orgId: number | null; employeeId: number | null };
  onClose: () => void;
  onSaved: () => void;
}

function ReceiverEditModal({ current, onClose, onSaved }: ReceiverEditModalProps) {
  const [orgId, setOrgId] = useState(current.orgId === null ? "" : String(current.orgId));
  const [employeeId, setEmployeeId] = useState(current.employeeId === null ? "" : String(current.employeeId));
  const [error, setError] = useState<string | undefined>(undefined);

  const mutation = useMutation({
    mutationFn: () => updateAutoImportReceiver({ org_unit_id: Number(orgId), employee_id: Number(employeeId) }),
    onSuccess: () => {
      toast.success("自動取込の受け皿を変更しました");
      onSaved();
    },
    onError: (e: Error) => toast.error(errorMessage(e, "受け皿の変更に失敗しました")),
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
      title="自動取込の受け皿を変更"
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
        有効な組織と、有効な社員だけを選べます。両方の設定が必要です。
      </p>
    </FormModal>
  );
}
