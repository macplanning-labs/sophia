"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Plus, ToggleLeft, ToggleRight, Pencil, FolderTree, Move } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/ui/page-header";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";
import { OrgAdminGate, OrgAdminTabs, errorMessage } from "@/components/org-admin/org-admin-shell";
import { OrgFormModal, MoveOrgModal, type OrgFormValues } from "@/components/org-admin/org-unit-dialogs";
import { AutoImportReceiverCard } from "@/components/org-admin/auto-import-receiver-card";
import {
  buildOrgTree,
  createOrgUnit,
  fetchOrgUnits,
  moveOrgUnit,
  setOrgUnitActive,
  updateOrgUnit,
  type OrgTreeNode,
  type OrgUnit,
} from "@/lib/org-admin";

type Dialog =
  | { kind: "create"; parent: OrgUnit }
  | { kind: "edit"; unit: OrgUnit }
  | { kind: "move"; unit: OrgUnit }
  | { kind: "active"; unit: OrgUnit; next: boolean }
  | null;

export default function OrgSettingsPage() {
  return (
    <OrgAdminGate>
      <OrgSettingsContent />
    </OrgAdminGate>
  );
}

function OrgSettingsContent() {
  const qc = useQueryClient();
  const [dialog, setDialog] = useState<Dialog>(null);

  const { data: units, isLoading, error } = useQuery({
    queryKey: ["org-units"],
    queryFn: fetchOrgUnits,
  });

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["org-units"] });
    qc.invalidateQueries({ queryKey: ["org-role-changes"] });
  };

  const createMutation = useMutation({
    mutationFn: (args: { parentId: number; values: OrgFormValues }) =>
      createOrgUnit({ parent_id: args.parentId, ...args.values }),
    onSuccess: () => {
      refresh();
      setDialog(null);
      toast.success("組織を追加しました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "組織の追加に失敗しました")),
  });

  const updateMutation = useMutation({
    mutationFn: (args: { id: number; values: OrgFormValues }) => updateOrgUnit(args.id, args.values),
    onSuccess: () => {
      refresh();
      setDialog(null);
      toast.success("組織を更新しました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "組織の更新に失敗しました")),
  });

  const moveMutation = useMutation({
    mutationFn: (args: { id: number; newParentId: number }) => moveOrgUnit(args.id, args.newParentId),
    onSuccess: () => {
      refresh();
      setDialog(null);
      toast.success("組織を付け替えました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "付け替えに失敗しました")),
  });

  const activeMutation = useMutation({
    mutationFn: (args: { id: number; isActive: boolean }) => setOrgUnitActive(args.id, args.isActive),
    onSuccess: (_data, args) => {
      refresh();
      setDialog(null);
      toast.success(args.isActive ? "組織を有効にしました" : "組織を無効にしました");
    },
    onError: (e: Error) => toast.error(errorMessage(e, "状態の変更に失敗しました")),
  });

  const busy = createMutation.isPending || updateMutation.isPending || moveMutation.isPending || activeMutation.isPending;
  const tree: OrgTreeNode[] = units ? buildOrgTree(units) : [];

  return (
    <div className="p-6 space-y-6">
      <PageHeader subtitle="組織の親子関係は、ルートから最大8段までです。" />

      <OrgAdminTabs active="org" />

      {error && (
        <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">
          {errorMessage(error, "組織の取得に失敗しました")}
        </div>
      )}

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">組織名</TableHead>
                <TableHead className="text-xs text-muted-foreground">種別</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">表示順</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">状態</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tree.length === 0 ? (
                <TableRow><TableCell colSpan={5} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : tree.map((n) => {
                const isRoot = n.parent_id === null;
                return (
                  <TableRow key={n.id} className={`border-border/50 ${n.is_active ? "" : "text-muted-foreground opacity-60"}`}>
                    <TableCell className="text-sm font-medium" style={{ paddingLeft: `${n.depth * 1.5 + 1}rem` }}>
                      <span className="inline-flex items-center gap-2">
                        {n.name}
                        {isRoot && <Badge variant="outline" className="text-[10px] border-border">ルート</Badge>}
                      </span>
                    </TableCell>
                    <TableCell className="text-[11px] text-muted-foreground">{n.unit_kind ?? "—"}</TableCell>
                    <TableCell className="text-center text-sm tabular-nums">{n.sort_order}</TableCell>
                    <TableCell className="text-center">
                      {n.is_active
                        ? <Badge className="bg-blue-500/20 text-blue-400 border-blue-500/30 text-[10px]">有効</Badge>
                        : <Badge variant="outline" className="border-border text-muted-foreground text-[10px]">無効</Badge>}
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap justify-end gap-2">
                        <Button
                          size="sm"
                          variant="outline"
                          className="border-border text-muted-foreground hover:text-foreground"
                          disabled={busy || !n.is_active}
                          onClick={() => setDialog({ kind: "create", parent: n })}
                        >
                          <Plus className="w-3.5 h-3.5 mr-1" /> 子を追加
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          className="border-border text-muted-foreground hover:text-foreground"
                          disabled={busy}
                          onClick={() => setDialog({ kind: "edit", unit: n })}
                        >
                          <Pencil className="w-3.5 h-3.5 mr-1" /> 編集
                        </Button>
                        {/* ルートは付け替え・無効化できない。ボタンは消さず、非活性にして理由を示す(行ごとに位置がずれない) */}
                        <Button
                          size="sm"
                          variant="outline"
                          className="border-border text-muted-foreground hover:text-foreground"
                          disabled={busy || isRoot}
                          title={isRoot ? "ルートの組織は、付け替えできません" : undefined}
                          onClick={() => setDialog({ kind: "move", unit: n })}
                        >
                          <Move className="w-3.5 h-3.5 mr-1" /> 付け替え
                        </Button>
                        <Button
                          size="sm"
                          variant="outline"
                          className="border-border text-muted-foreground hover:text-foreground"
                          disabled={busy || isRoot}
                          title={isRoot ? "ルートの組織は、無効にできません" : undefined}
                          onClick={() => setDialog({ kind: "active", unit: n, next: !n.is_active })}
                        >
                          {n.is_active
                            ? <><ToggleRight className="w-3.5 h-3.5 mr-1 text-emerald-400" /> 無効にする</>
                            : <><ToggleLeft className="w-3.5 h-3.5 mr-1" /> 有効にする</>}
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60 flex items-center gap-2">
          <FolderTree className="w-3.5 h-3.5 text-muted-foreground" />
          <span className="text-xs text-muted-foreground">表示中: {tree.length}件（無効な組織も含みます）</span>
        </div>
      </div>

      <AutoImportReceiverCard />

      {dialog?.kind === "create" && (
        <OrgFormModal
          key={`create-${dialog.parent.id}`}
          mode="create"
          parentName={dialog.parent.name}
          loading={createMutation.isPending}
          onClose={() => setDialog(null)}
          onSubmit={(values) => createMutation.mutate({ parentId: dialog.parent.id, values })}
        />
      )}

      {dialog?.kind === "edit" && (
        <OrgFormModal
          key={`edit-${dialog.unit.id}`}
          mode="edit"
          initial={dialog.unit}
          loading={updateMutation.isPending}
          onClose={() => setDialog(null)}
          onSubmit={(values) => updateMutation.mutate({ id: dialog.unit.id, values })}
        />
      )}

      {dialog?.kind === "move" && units && (
        <MoveOrgModal
          key={`move-${dialog.unit.id}`}
          target={dialog.unit}
          units={units}
          loading={moveMutation.isPending}
          onClose={() => setDialog(null)}
          onSubmit={(newParentId) => moveMutation.mutate({ id: dialog.unit.id, newParentId })}
        />
      )}

      <ConfirmSheet
        open={dialog?.kind === "active"}
        title={dialog?.kind === "active" && dialog.next ? "組織を有効にする" : "組織を無効にする"}
        facts={dialog?.kind === "active" ? [{ label: "組織名", value: dialog.unit.name }] : undefined}
        description={
          dialog?.kind === "active" && dialog.next
            ? "この組織を有効に戻します。割り当ては、もう一度ご確認ください。"
            : "無効にすると、この組織には新しい割り当てができなくなります。配下の組織も一覧では無効のまま残ります。"
        }
        variant={dialog?.kind === "active" && !dialog.next ? "danger" : "default"}
        confirmLabel={dialog?.kind === "active" && dialog.next ? "有効にする" : "無効にする"}
        loading={activeMutation.isPending}
        onConfirm={() => {
          if (dialog?.kind === "active") activeMutation.mutate({ id: dialog.unit.id, isActive: dialog.next });
        }}
        onCancel={() => setDialog(null)}
      />
    </div>
  );
}
