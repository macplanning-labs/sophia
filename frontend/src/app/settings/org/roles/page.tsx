"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";
import { OrgAdminGate, OrgAdminTabs, errorMessage } from "@/components/org-admin/org-admin-shell";
import { RoleFormModal, type RoleFormValues } from "@/components/org-admin/role-form-modal";
import { PermissionMatrix } from "@/components/org-admin/permission-matrix";
import {
  createRole,
  deleteRole,
  fetchPermissionCatalog,
  fetchRolePermissions,
  fetchRoles,
  renameRole,
  type RoleRow,
} from "@/lib/org-admin";

type Dialog =
  | { kind: "create" }
  | { kind: "rename"; role: RoleRow }
  | { kind: "delete"; role: RoleRow }
  | null;

export default function OrgRolesPage() {
  return (
    <OrgAdminGate>
      <OrgRolesContent />
    </OrgAdminGate>
  );
}

function OrgRolesContent() {
  const qc = useQueryClient();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);

  const rolesQuery = useQuery({ queryKey: ["org-roles"], queryFn: fetchRoles });
  const catalogQuery = useQuery({ queryKey: ["org-permission-catalog"], queryFn: fetchPermissionCatalog });
  const roles = rolesQuery.data ?? [];
  const activeId = selectedId ?? roles[0]?.id ?? null;
  const activeRole = roles.find((r) => r.id === activeId) ?? null;

  const permsQuery = useQuery({
    queryKey: ["org-role-permissions", activeId],
    queryFn: () => fetchRolePermissions(activeId as number),
    enabled: activeId !== null,
  });

  const refreshRoles = () => {
    qc.invalidateQueries({ queryKey: ["org-roles"] });
    qc.invalidateQueries({ queryKey: ["org-role-changes"] });
  };

  const createMutation = useMutation({
    mutationFn: (v: RoleFormValues) => createRole(v),
    onSuccess: (role) => {
      refreshRoles();
      setSelectedId(role.id);
      setDialog(null);
      toast.success("ロールを追加しました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "ロールの追加に失敗しました")),
  });

  const renameMutation = useMutation({
    mutationFn: (args: { id: number; name: string }) => renameRole(args.id, args.name),
    onSuccess: () => {
      refreshRoles();
      setDialog(null);
      toast.success("ロール名を変更しました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "ロール名の変更に失敗しました")),
  });

  const deleteMutation = useMutation({
    mutationFn: (id: number) => deleteRole(id),
    onSuccess: () => {
      refreshRoles();
      setSelectedId(null);
      setDialog(null);
      toast.success("ロールを削除しました");
    },
    onError: (e: Error) => {
      setDialog(null);
      toast.error(errorMessage(e, "ロールの削除に失敗しました"));
    },
  });

  return (
    <div className="p-6 space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <OrgAdminTabs active="roles" />
        <Button size="sm" onClick={() => setDialog({ kind: "create" })}>
          <Plus className="w-4 h-4 mr-1" /> ロールを追加
        </Button>
      </div>

      {(rolesQuery.error || catalogQuery.error) && (
        <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {errorMessage(rolesQuery.error ?? catalogQuery.error, "ロールの取得に失敗しました")}
        </div>
      )}

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {rolesQuery.isLoading ? (
          <div className="p-8 space-y-3">{[...Array(3)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">ロール名</TableHead>
                <TableHead className="text-xs text-muted-foreground">コード</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">区分</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">割り当て数</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {roles.length === 0 ? (
                <TableRow><TableCell colSpan={5} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : roles.map((r) => (
                <TableRow
                  key={r.id}
                  data-state={r.id === activeId ? "selected" : undefined}
                  className={`border-border/50 cursor-pointer hover:bg-accent/50 ${r.id === activeId ? "bg-accent/40" : ""}`}
                  onClick={() => setSelectedId(r.id)}
                >
                  <TableCell className="text-sm font-medium">{r.name}</TableCell>
                  <TableCell className="text-[11px] text-muted-foreground font-mono">{r.code}</TableCell>
                  <TableCell className="text-center">
                    {r.is_system
                      ? <Badge variant="outline" className="text-[10px] border-border">標準</Badge>
                      : <Badge variant="outline" className="text-[10px] border-border text-muted-foreground">追加</Badge>}
                  </TableCell>
                  <TableCell className="text-center text-sm tabular-nums">{r.assignment_count}</TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-2" onClick={(e) => e.stopPropagation()}>
                      <Button
                        size="sm"
                        variant="outline"
                        className="border-border text-muted-foreground hover:text-foreground"
                        onClick={() => setDialog({ kind: "rename", role: r })}
                      >
                        <Pencil className="w-3.5 h-3.5 mr-1" /> 名前変更
                      </Button>
                      {/* 削除は消さず、システムの役割・割り当てが残る役割では非活性にする(行ごとに位置がずれない) */}
                      <Button
                        size="sm"
                        variant="outline"
                        className="border-border text-muted-foreground hover:text-red-400"
                        disabled={r.is_system || r.assignment_count > 0}
                        title={r.is_system ? "システムの役割は削除できません" : r.assignment_count > 0 ? "割り当てが残っているため削除できません" : undefined}
                        onClick={() => setDialog({ kind: "delete", role: r })}
                      >
                        <Trash2 className="w-3.5 h-3.5 mr-1" /> 削除
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {roles.length}件。行をクリックすると、その権限を編集できます。</span>
        </div>
      </div>

      {activeRole && catalogQuery.data && permsQuery.data && (
        <PermissionMatrix
          key={activeRole.id}
          roleId={activeRole.id}
          roleName={activeRole.name}
          catalog={catalogQuery.data}
          saved={permsQuery.data.permissions}
        />
      )}
      {activeRole && permsQuery.isLoading && (
        <div className="animate-pulse h-40 bg-muted/50 rounded-lg" />
      )}

      {dialog?.kind === "create" && (
        <RoleFormModal mode="create" loading={createMutation.isPending} onClose={() => setDialog(null)} onSubmit={(v) => createMutation.mutate(v)} />
      )}
      {dialog?.kind === "rename" && (
        <RoleFormModal
          key={dialog.role.id}
          mode="rename"
          initial={{ code: dialog.role.code, name: dialog.role.name }}
          loading={renameMutation.isPending}
          onClose={() => setDialog(null)}
          onSubmit={(v) => renameMutation.mutate({ id: dialog.role.id, name: v.name })}
        />
      )}

      <ConfirmSheet
        open={dialog?.kind === "delete"}
        title="ロールを削除"
        facts={dialog?.kind === "delete" ? [{ label: "ロール名", value: dialog.role.name }] : undefined}
        description="この操作は取り消せません。ロールに設定した権限も消えます。"
        variant="danger"
        confirmLabel="削除"
        loading={deleteMutation.isPending}
        onConfirm={() => {
          if (dialog?.kind === "delete") deleteMutation.mutate(dialog.role.id);
        }}
        onCancel={() => setDialog(null)}
      />
    </div>
  );
}
