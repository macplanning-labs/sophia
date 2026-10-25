"use client";

import { useMemo, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";
import { errorMessage } from "@/components/org-admin/org-admin-shell";
import {
  DOMAIN_LABEL,
  SCOPE_LABEL,
  draftFromServer,
  draftToPayload,
  findNewlyGrantedSensitive,
  groupPermissionsByDomain,
  isSameGrants,
  replaceRolePermissions,
  type PermissionCatalogItem,
  type PermissionDraft,
  type RolePermission,
} from "@/lib/org-admin";

interface PermissionMatrixProps {
  roleId: number;
  roleName: string;
  catalog: PermissionCatalogItem[];
  /** サーバーに保存されている権限セット */
  saved: RolePermission[];
}

/**
 * ロールの権限セットを編集する表。呼び出し側は roleId を key にして、ロールを切り替えるたびに作り直す。
 * 保存の前に、今まで付いていなかった機微な権限があれば確認を出す。
 */
export function PermissionMatrix({ roleId, roleName, catalog, saved }: PermissionMatrixProps) {
  const qc = useQueryClient();
  const [draft, setDraft] = useState<PermissionDraft>(() => draftFromServer(catalog, saved));
  const [pendingSensitive, setPendingSensitive] = useState<string[] | null>(null);
  const groups = useMemo(() => groupPermissionsByDomain(catalog), [catalog]);
  const labelByCode = useMemo(() => new Map(catalog.map((c) => [c.code, c.label_ja])), [catalog]);

  const payload = draftToPayload(catalog, draft);
  const dirty = !isSameGrants(payload, saved);

  const saveMutation = useMutation({
    mutationFn: (perms: RolePermission[]) => replaceRolePermissions(roleId, perms),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["org-role-permissions", roleId] });
      qc.invalidateQueries({ queryKey: ["org-roles"] });
      qc.invalidateQueries({ queryKey: ["org-role-changes"] });
      setPendingSensitive(null);
      toast.success("権限を保存しました");
    },
    onError: (e: Error) => {
      setPendingSensitive(null);
      toast.error(errorMessage(e, "権限の保存に失敗しました"));
    },
  });

  const update = (code: string, patch: Partial<{ granted: boolean; scope: PermissionDraft[string]["scope"] }>) => {
    setDraft((d) => ({ ...d, [code]: { ...d[code], ...patch } }));
  };

  const handleSave = () => {
    const newly = findNewlyGrantedSensitive(saved, payload, catalog);
    if (newly.length > 0) {
      setPendingSensitive(newly);
      return;
    }
    saveMutation.mutate(payload);
  };

  return (
    <div className="bg-card border border-border rounded-lg overflow-hidden">
      <div className="px-4 py-3 border-b border-border flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium">「{roleName}」の権限</p>
          <p className="text-[11px] text-muted-foreground">
            チェックを入れると付与されます。範囲は、その権限で見られるデータの広さです。
          </p>
        </div>
        <Button size="sm" onClick={handleSave} disabled={!dirty || saveMutation.isPending}>
          {saveMutation.isPending ? "保存中..." : "権限を保存"}
        </Button>
      </div>

      <Table>
        <TableHeader>
          <TableRow className="border-border hover:bg-transparent">
            <TableHead className="text-xs text-muted-foreground text-center w-16">付与</TableHead>
            <TableHead className="text-xs text-muted-foreground">権限</TableHead>
            <TableHead className="text-xs text-muted-foreground">範囲</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {groups.map((g) => (
            <GroupRows key={g.domain} domain={g.domain} items={g.items} draft={draft} onChange={update} />
          ))}
        </TableBody>
      </Table>

      <ConfirmSheet
        open={pendingSensitive !== null}
        title="機微な権限を付与します"
        facts={[
          {
            label: "新たに付与する権限",
            value: (pendingSensitive ?? []).map((code) => labelByCode.get(code) ?? code).join("、"),
          },
        ]}
        description="機微な権限は、給与や個人情報など見せてはいけない情報に関わります。このロールを持つ人は、すぐに該当の情報を見られるようになります。"
        variant="danger"
        confirmLabel="付与して保存"
        loading={saveMutation.isPending}
        onConfirm={() => saveMutation.mutate(payload)}
        onCancel={() => setPendingSensitive(null)}
      />
    </div>
  );
}

function GroupRows({
  domain,
  items,
  draft,
  onChange,
}: {
  domain: string;
  items: PermissionCatalogItem[];
  draft: PermissionDraft;
  onChange: (code: string, patch: Partial<{ granted: boolean; scope: PermissionDraft[string]["scope"] }>) => void;
}) {
  return (
    <>
      <TableRow className="border-border/50 bg-muted/30 hover:bg-muted/30">
        <TableCell colSpan={3} className="text-xs font-medium text-muted-foreground">
          {DOMAIN_LABEL[domain] ?? domain}
        </TableCell>
      </TableRow>
      {items.map((item) => {
        const d = draft[item.code];
        return (
          <TableRow key={item.code} className="border-border/50">
            <TableCell className="text-center">
              <input
                type="checkbox"
                aria-label={`${item.label_ja}を付与`}
                checked={d.granted}
                onChange={(e) => onChange(item.code, { granted: e.target.checked })}
                className="rounded border-border"
              />
            </TableCell>
            <TableCell>
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm">{item.label_ja}</span>
                {item.is_sensitive && (
                  <Badge className="bg-amber-500/20 text-amber-400 border-amber-500/30 text-[10px]">機微</Badge>
                )}
              </div>
              <div className="text-[11px] text-muted-foreground font-mono">{item.code}</div>
            </TableCell>
            <TableCell>
              <select
                aria-label={`${item.label_ja}の範囲`}
                value={d.scope}
                disabled={!d.granted || item.allowed_scopes.length <= 1}
                onChange={(e) => onChange(item.code, { scope: e.target.value as PermissionDraft[string]["scope"] })}
                className="px-2 py-1 bg-muted border border-border rounded-md text-sm text-foreground disabled:opacity-50"
              >
                {item.allowed_scopes.map((s) => (
                  <option key={s} value={s}>{SCOPE_LABEL[s]}</option>
                ))}
              </select>
              {!item.is_stamped && <p className="text-[10px] text-muted-foreground mt-1">この権限は全社のみ</p>}
            </TableCell>
          </TableRow>
        );
      })}
    </>
  );
}
