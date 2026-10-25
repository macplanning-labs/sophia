// lib/org-admin.ts — 組織・ロール・割り当ての管理画面（/settings/org 配下）の型・取得/更新関数・純粋な補助関数。
//
// バックエンド: src/presentation/handlers/org_admin/*.rs（DEMO-000173 PR 5a）。Admin 専用。
// エラー本文は {"error": "日本語のメッセージ"}。api.ts の fetchJson が message として投げる。

import { apiDelete, apiGet, apiPost, apiPut } from "@/lib/api";

// ── 型 ──

export type OrgScope = "PRIVATE" | "UNIT" | "HIERARCHICAL" | "GLOBAL";

export const SCOPE_LABEL: Record<OrgScope, string> = {
  PRIVATE: "本人限定",
  UNIT: "自組織",
  HIERARCHICAL: "組織配下",
  GLOBAL: "全社",
};

/** 権限のドメイン（権限コードの先頭）を画面用の日本語にする。未知のドメインはそのまま出す。 */
/** 変更履歴の対象の表示名。サーバ(h_org_role_change.target_kind)の種類をすべて日本語にする。 */
export const CHANGE_TARGET_LABEL: Record<string, string> = {
  org_unit: "組織",
  role: "ロール",
  role_permission: "ロールの権限",
  assignment: "割り当て",
  owner_transfer: "担当の付け替え",
  auto_import_receiver: "自動取込の受け皿",
};

/** 変更履歴の操作の表示名。 */
export const CHANGE_ACTION_LABEL: Record<string, string> = {
  create: "追加",
  update: "変更",
  move: "付け替え",
  delete: "削除",
  transfer: "担当を変更",
};

/** 対象の番号を出すか。会社設定(自動取込の受け皿)は1件しかなく、番号に意味が無いので出さない。 */
export function showChangeTargetId(c: { target_kind: string; target_id: number | null }): boolean {
  return c.target_id !== null && c.target_kind !== "auto_import_receiver";
}

export const DOMAIN_LABEL: Record<string, string> = {
  billing: "請求",
  client: "取引先（受注）",
  contract: "契約",
  engineer: "要員",
  expense: "経費",
  hr: "人事",
  mail: "受信メール",
  order: "注文",
  partner: "取引先（発注）",
  payroll: "給与",
  project: "案件",
  settings: "設定",
  system: "システム",
  timesheet: "勤務表",
};

export interface OrgUnit {
  id: number;
  parent_id: number | null;
  name: string;
  unit_kind: string | null;
  sort_order: number;
  is_active: boolean;
}

/** 木の表示用。depth は根が 0。 */
export interface OrgTreeNode extends OrgUnit {
  depth: number;
}

export interface PermissionCatalogItem {
  code: string;
  label_ja: string;
  domain: string;
  is_stamped: boolean;
  is_sensitive: boolean;
  allowed_scopes: OrgScope[];
}

export interface RoleRow {
  id: number;
  code: string;
  name: string;
  is_system: boolean;
  assignment_count: number;
}

export interface RolePermission {
  permission_code: string;
  scope: OrgScope;
}

export interface RoleDetail {
  id: number;
  code: string;
  name: string;
  is_system: boolean;
}

export interface RolePermissionsResponse {
  role: RoleDetail;
  permissions: RolePermission[];
}

export interface AssignmentRow {
  id: number;
  user_id: number;
  user_email: string;
  username: string;
  org_unit_id: number;
  org_unit_name: string;
  role_id: number;
  role_code: string;
  role_name: string;
  created_at: string;
  created_by: number | null;
}

export interface OrgRoleChange {
  id: number;
  changed_at: string;
  changed_by: number | null;
  changed_by_name: string | null;
  target_kind: string;
  target_id: number | null;
  action: string;
  before_json: unknown;
  after_json: unknown;
}

/** 権限セットの編集画面で使う下書き。権限コードごとに、付与の有無と範囲を持つ。 */
export type PermissionDraft = Record<string, { granted: boolean; scope: OrgScope }>;

// ── 取得・更新 ──

export async function fetchOrgUnits(): Promise<OrgUnit[]> {
  const res = await apiGet<{ org_units: OrgUnit[] }>("/api/v1/admin/org-units");
  return res.org_units;
}

export interface OrgUnitInput {
  parent_id?: number | null;
  name: string;
  unit_kind?: string | null;
  sort_order?: number;
}

export async function createOrgUnit(input: OrgUnitInput & { parent_id: number }): Promise<OrgUnit> {
  const res = await apiPost<{ org_unit: OrgUnit }>("/api/v1/admin/org-units", input);
  return res.org_unit;
}

export async function updateOrgUnit(id: number, input: { name: string; unit_kind?: string | null; sort_order?: number }): Promise<OrgUnit> {
  const res = await apiPut<{ org_unit: OrgUnit }>(`/api/v1/admin/org-units/${id}`, input);
  return res.org_unit;
}

export async function moveOrgUnit(id: number, newParentId: number): Promise<OrgUnit> {
  const res = await apiPost<{ org_unit: OrgUnit }>(`/api/v1/admin/org-units/${id}/move`, { new_parent_id: newParentId });
  return res.org_unit;
}

export async function setOrgUnitActive(id: number, isActive: boolean): Promise<OrgUnit> {
  const res = await apiPost<{ org_unit: OrgUnit }>(`/api/v1/admin/org-units/${id}/active`, { is_active: isActive });
  return res.org_unit;
}

export async function fetchPermissionCatalog(): Promise<PermissionCatalogItem[]> {
  const res = await apiGet<{ permissions: PermissionCatalogItem[] }>("/api/v1/admin/permissions");
  return res.permissions;
}

export async function fetchRoles(): Promise<RoleRow[]> {
  const res = await apiGet<{ roles: RoleRow[] }>("/api/v1/admin/roles");
  return res.roles;
}

export async function createRole(input: { code: string; name: string }): Promise<RoleRow> {
  const res = await apiPost<{ role: RoleRow }>("/api/v1/admin/roles", input);
  return res.role;
}

export async function renameRole(id: number, name: string): Promise<RoleDetail> {
  const res = await apiPut<{ role: RoleDetail }>(`/api/v1/admin/roles/${id}`, { name });
  return res.role;
}

export async function deleteRole(id: number): Promise<void> {
  await apiDelete(`/api/v1/admin/roles/${id}`);
}

export async function fetchRolePermissions(id: number): Promise<RolePermissionsResponse> {
  return apiGet<RolePermissionsResponse>(`/api/v1/admin/roles/${id}/permissions`);
}

export async function replaceRolePermissions(id: number, permissions: RolePermission[]): Promise<RolePermissionsResponse> {
  return apiPut<RolePermissionsResponse>(`/api/v1/admin/roles/${id}/permissions`, { permissions });
}

export async function fetchAssignments(): Promise<AssignmentRow[]> {
  const res = await apiGet<{ assignments: AssignmentRow[] }>("/api/v1/admin/assignments");
  return res.assignments;
}

export async function createAssignment(input: { user_id: number; org_unit_id: number; role_id: number }): Promise<AssignmentRow> {
  const res = await apiPost<{ assignment: AssignmentRow }>("/api/v1/admin/assignments", input);
  return res.assignment;
}

export async function deleteAssignment(id: number): Promise<void> {
  await apiDelete(`/api/v1/admin/assignments/${id}`);
}

export async function fetchOrgRoleChanges(limit = 100): Promise<OrgRoleChange[]> {
  const res = await apiGet<{ changes: OrgRoleChange[] }>(`/api/v1/admin/org-role-changes?limit=${limit}`);
  return res.changes;
}

// ── 純粋な補助関数 ──

/**
 * 組織の木を、表示順（sort_order → id。バックエンドの並びと同じ）で深さ優先に並べる。
 * 親が見つからない組織は根として扱い、欠けずに表示する。
 */
export function buildOrgTree(units: OrgUnit[]): OrgTreeNode[] {
  const byOrder = (a: OrgUnit, b: OrgUnit) => a.sort_order - b.sort_order || a.id - b.id;
  const ids = new Set(units.map((u) => u.id));
  const children = new Map<number, OrgUnit[]>();
  const roots: OrgUnit[] = [];
  for (const u of units) {
    if (u.parent_id === null || !ids.has(u.parent_id)) {
      roots.push(u);
    } else {
      const list = children.get(u.parent_id) ?? [];
      list.push(u);
      children.set(u.parent_id, list);
    }
  }

  const result: OrgTreeNode[] = [];
  const visited = new Set<number>();
  const visit = (unit: OrgUnit, depth: number) => {
    if (visited.has(unit.id)) return;
    visited.add(unit.id);
    result.push({ ...unit, depth });
    for (const child of [...(children.get(unit.id) ?? [])].sort(byOrder)) {
      visit(child, depth + 1);
    }
  };
  for (const root of [...roots].sort(byOrder)) visit(root, 0);
  return result;
}

/** 指定の組織と、その配下のすべての組織の ID。付け替え先の候補から外すのに使う。 */
export function descendantIds(units: OrgUnit[], id: number): Set<number> {
  const result = new Set<number>([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const u of units) {
      if (u.parent_id !== null && result.has(u.parent_id) && !result.has(u.id)) {
        result.add(u.id);
        grew = true;
      }
    }
  }
  return result;
}

export interface PermissionDomainGroup {
  domain: string;
  items: PermissionCatalogItem[];
}

/** 権限カタログをドメインごとにまとめる。ドメインの並びは、カタログに最初に現れた順。 */
export function groupPermissionsByDomain(catalog: PermissionCatalogItem[]): PermissionDomainGroup[] {
  const groups: PermissionDomainGroup[] = [];
  const index = new Map<string, PermissionDomainGroup>();
  for (const item of catalog) {
    let group = index.get(item.domain);
    if (!group) {
      group = { domain: item.domain, items: [] };
      index.set(item.domain, group);
      groups.push(group);
    }
    group.items.push(item);
  }
  return groups;
}

/**
 * 保存しようとしている権限セットのうち、今まで付いていなかった機微な権限のコードを返す。
 * 既に付いていた権限の範囲だけを変えた場合は含めない。
 */
export function findNewlyGrantedSensitive(
  before: RolePermission[],
  after: RolePermission[],
  catalog: PermissionCatalogItem[],
): string[] {
  // 新しく付けたものに加えて、範囲を広げたもの(例: 本人限定 → 全社)も、確認の対象にする。
  const beforeScope = new Map(before.map((p) => [p.permission_code, p.scope]));
  const sensitive = new Set(catalog.filter((c) => c.is_sensitive).map((c) => c.code));
  return after
    .filter((p) => {
      if (!sensitive.has(p.permission_code)) return false;
      const prev = beforeScope.get(p.permission_code);
      return prev === undefined || SCOPE_RANK[p.scope] > SCOPE_RANK[prev];
    })
    .map((p) => p.permission_code);
}

/** 範囲の広さの順(広いほど大きい)。本人限定と自組織は軸が違うが、確認の要否の判定では本人限定を最も狭いとみなす。 */
const SCOPE_RANK: Record<OrgScope, number> = { PRIVATE: 0, UNIT: 1, HIERARCHICAL: 2, GLOBAL: 3 };

/** 無効な組織と、その配下(子・孫…)の組織ID。配下の組織への割り当ては権限として働かない(サーバと同じ規則)。 */
export function disabledOrgIds(units: OrgUnit[]): Set<number> {
  const byId = new Map(units.map((u) => [u.id, u]));
  const disabled = new Set<number>();
  for (const u of units) {
    let cur: OrgUnit | undefined = u;
    let steps = 0;
    while (cur && steps <= units.length) {
      if (!cur.is_active) {
        disabled.add(u.id);
        break;
      }
      cur = cur.parent_id === null ? undefined : byId.get(cur.parent_id);
      steps += 1;
    }
  }
  return disabled;
}

/** 権限カタログから、各権限の初期の範囲（スタンプ型は自組織、それ以外は全社）を決める。 */
export function defaultScopeFor(item: PermissionCatalogItem): OrgScope {
  if (item.allowed_scopes.includes("UNIT")) return "UNIT";
  return item.allowed_scopes[0] ?? "GLOBAL";
}

/** サーバーの権限セットから、編集用の下書きを作る。 */
export function draftFromServer(catalog: PermissionCatalogItem[], saved: RolePermission[]): PermissionDraft {
  const savedByCode = new Map(saved.map((p) => [p.permission_code, p.scope]));
  const draft: PermissionDraft = {};
  for (const item of catalog) {
    const scope = savedByCode.get(item.code);
    draft[item.code] = {
      granted: scope !== undefined,
      scope: scope ?? defaultScopeFor(item),
    };
  }
  return draft;
}

/** 下書きのうち付与されているものだけを、保存用の一覧にする（カタログの順）。 */
export function draftToPayload(catalog: PermissionCatalogItem[], draft: PermissionDraft): RolePermission[] {
  return catalog
    .filter((item) => draft[item.code]?.granted)
    .map((item) => ({ permission_code: item.code, scope: draft[item.code].scope }));
}

/** 2つの権限セットが同じか（順序は問わない）。 */
export function isSameGrants(a: RolePermission[], b: RolePermission[]): boolean {
  if (a.length !== b.length) return false;
  const key = (p: RolePermission) => `${p.permission_code}:${p.scope}`;
  const set = new Set(a.map(key));
  return b.every((p) => set.has(key(p)));
}
