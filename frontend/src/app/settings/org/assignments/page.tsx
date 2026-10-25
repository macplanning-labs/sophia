"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";
import { fetchUsers } from "@/lib/api";
import { OrgAdminGate, OrgAdminTabs, errorMessage } from "@/components/org-admin/org-admin-shell";
import { AssignmentFormModal, type AssignmentFormValues } from "@/components/org-admin/assignment-form-modal";
import { ChangeLogTable } from "@/components/org-admin/change-log-table";
import {
  createAssignment,
  deleteAssignment,
  fetchAssignments,
  fetchOrgRoleChanges,
  fetchOrgUnits,
  fetchRoles,
  type AssignmentRow,
} from "@/lib/org-admin";

export default function OrgAssignmentsPage() {
  return (
    <OrgAdminGate>
      <OrgAssignmentsContent />
    </OrgAdminGate>
  );
}

function OrgAssignmentsContent() {
  const qc = useQueryClient();
  const [formOpen, setFormOpen] = useState(false);
  const [deleting, setDeleting] = useState<AssignmentRow | null>(null);

  const assignmentsQuery = useQuery({ queryKey: ["org-assignments"], queryFn: fetchAssignments });
  const unitsQuery = useQuery({ queryKey: ["org-units"], queryFn: fetchOrgUnits });
  const rolesQuery = useQuery({ queryKey: ["org-roles"], queryFn: fetchRoles });
  const usersQuery = useQuery({ queryKey: ["users"], queryFn: fetchUsers });
  const changesQuery = useQuery({ queryKey: ["org-role-changes"], queryFn: () => fetchOrgRoleChanges(100) });

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["org-assignments"] });
    qc.invalidateQueries({ queryKey: ["org-roles"] });
    qc.invalidateQueries({ queryKey: ["org-role-changes"] });
  };

  const createMutation = useMutation({
    mutationFn: (v: AssignmentFormValues) => createAssignment(v),
    onSuccess: () => {
      refresh();
      setFormOpen(false);
      toast.success("割り当てを追加しました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "割り当ての追加に失敗しました")),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteAssignment(id),
    onSuccess: () => {
      refresh();
      setDeleting(null);
      toast.success("割り当てを外しました");
    },
    onError: (e: Error) => {
      setDeleting(null);
      toast.error(errorMessage(e, "割り当ての削除に失敗しました"));
    },
  });

  const assignments = assignmentsQuery.data ?? [];
  const loadError = assignmentsQuery.error ?? unitsQuery.error ?? rolesQuery.error ?? usersQuery.error;

  return (
    <div className="p-6 space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <OrgAdminTabs active="assignments" />
        <Button
          size="sm"
          onClick={() => setFormOpen(true)}
          disabled={!unitsQuery.data || !rolesQuery.data || !usersQuery.data}
        >
          <Plus className="w-4 h-4 mr-1" /> 割り当てを追加
        </Button>
      </div>

      {loadError && (
        <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {errorMessage(loadError, "割り当ての取得に失敗しました")}
        </div>
      )}

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {assignmentsQuery.isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">ユーザー</TableHead>
                <TableHead className="text-xs text-muted-foreground">組織</TableHead>
                <TableHead className="text-xs text-muted-foreground">ロール</TableHead>
                <TableHead className="text-xs text-muted-foreground">作成日</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {assignments.length === 0 ? (
                <TableRow><TableCell colSpan={5} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : assignments.map((a) => (
                <TableRow key={a.id} className="border-border/50">
                  <TableCell>
                    <div className="text-sm font-medium">{a.username}</div>
                    <div className="text-[11px] text-muted-foreground">{a.user_email}</div>
                  </TableCell>
                  <TableCell className="text-sm">{a.org_unit_name}</TableCell>
                  <TableCell>
                    <div className="text-sm">{a.role_name}</div>
                    <div className="text-[11px] text-muted-foreground font-mono">{a.role_code}</div>
                  </TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">{a.created_at?.slice(0, 10)}</TableCell>
                  <TableCell className="text-right">
                    <Button
                      size="sm"
                      variant="outline"
                      className="border-border text-muted-foreground hover:text-red-400"
                      disabled={deleteMutation.isPending}
                      onClick={() => setDeleting(a)}
                    >
                      <Trash2 className="w-3.5 h-3.5 mr-1" /> 外す
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {assignments.length}件</span>
        </div>
      </div>

      <ChangeLogTable changes={changesQuery.data} loading={changesQuery.isLoading} />

      {formOpen && unitsQuery.data && rolesQuery.data && usersQuery.data && (
        <AssignmentFormModal
          users={usersQuery.data}
          units={unitsQuery.data}
          roles={rolesQuery.data}
          loading={createMutation.isPending}
          onClose={() => setFormOpen(false)}
          onSubmit={(v) => createMutation.mutate(v)}
        />
      )}

      <ConfirmSheet
        open={deleting !== null}
        title="割り当てを外す"
        facts={deleting ? [
          { label: "ユーザー", value: deleting.username },
          { label: "組織", value: deleting.org_unit_name },
          { label: "ロール", value: deleting.role_name },
        ] : undefined}
        description="この割り当てを外します。外すと、その組織とロールで使えていた権限はなくなります。"
        variant="danger"
        confirmLabel="外す"
        loading={deleteMutation.isPending}
        onConfirm={() => {
          if (deleting) deleteMutation.mutate(deleting.id);
        }}
        onCancel={() => setDeleting(null)}
      />
    </div>
  );
}
