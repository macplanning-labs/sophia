"use client";

import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { CHANGE_ACTION_LABEL, CHANGE_TARGET_LABEL, showChangeTargetId, type OrgRoleChange } from "@/lib/org-admin";

function formatDateTime(value: string): string {
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? value : d.toLocaleString("ja-JP");
}

/** 組織・ロール・割り当ての変更履歴（新しい順）。 */
export function ChangeLogTable({ changes, loading }: { changes: OrgRoleChange[] | undefined; loading: boolean }) {
  return (
    <div className="bg-card border border-border rounded-lg overflow-hidden">
      <div className="px-4 py-3 border-b border-border">
        <p className="text-sm font-medium">変更履歴</p>
        <p className="text-[11px] text-muted-foreground">直近100件。誰が・いつ・何を変えたかを残しています。</p>
      </div>
      {loading ? (
        <div className="p-8 space-y-3">{[...Array(3)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
      ) : (
        <Table>
          <TableHeader>
            <TableRow className="border-border hover:bg-transparent">
              <TableHead className="text-xs text-muted-foreground">日時</TableHead>
              <TableHead className="text-xs text-muted-foreground">操作者</TableHead>
              <TableHead className="text-xs text-muted-foreground">対象</TableHead>
              <TableHead className="text-xs text-muted-foreground">操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {(changes ?? []).length === 0 ? (
              <TableRow><TableCell colSpan={4} className="text-center py-10 text-muted-foreground">変更履歴はありません</TableCell></TableRow>
            ) : changes?.map((c) => (
              <TableRow key={c.id} className="border-border/50">
                <TableCell className="text-[11px] text-muted-foreground tabular-nums whitespace-nowrap">{formatDateTime(c.changed_at)}</TableCell>
                <TableCell className="text-sm">{c.changed_by_name ?? "—"}</TableCell>
                <TableCell className="text-sm">
                  {CHANGE_TARGET_LABEL[c.target_kind] ?? c.target_kind}
                  {showChangeTargetId(c) && <span className="text-[11px] text-muted-foreground"> #{c.target_id}</span>}
                </TableCell>
                <TableCell className="text-sm">{CHANGE_ACTION_LABEL[c.action] ?? c.action}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );
}
