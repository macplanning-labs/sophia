import { describe, it, expect } from "vitest";
import {
  buildOrgTree,
  descendantIds,
  groupPermissionsByDomain,
  findNewlyGrantedSensitive,
  disabledOrgIds,
  CHANGE_TARGET_LABEL,
  CHANGE_ACTION_LABEL,
  showChangeTargetId,
  draftFromServer,
  draftToPayload,
  isSameGrants,
  type OrgUnit,
  type PermissionCatalogItem,
} from "../org-admin";

const unit = (id: number, parent_id: number | null, sort_order = 0, is_active = true): OrgUnit => ({
  id,
  parent_id,
  name: `組織${id}`,
  unit_kind: null,
  sort_order,
  is_active,
});

const perm = (
  code: string,
  domain: string,
  opts: Partial<PermissionCatalogItem> = {},
): PermissionCatalogItem => ({
  code,
  label_ja: code,
  domain,
  is_stamped: false,
  is_sensitive: false,
  allowed_scopes: ["GLOBAL"],
  ...opts,
});

describe("buildOrgTree", () => {
  it("根を0段目にして、子は深さを1つずつ増やす", () => {
    const tree = buildOrgTree([unit(1, null), unit(2, 1), unit(3, 2)]);
    expect(tree.map((n) => [n.id, n.depth])).toEqual([
      [1, 0],
      [2, 1],
      [3, 2],
    ]);
  });

  it("兄弟は sort_order、同じ順なら id の順に並べ、子は親の直後に続ける", () => {
    const tree = buildOrgTree([
      unit(1, null),
      unit(5, 1, 2),
      unit(4, 1, 1),
      unit(6, 1, 1),
      unit(7, 4, 0),
    ]);
    expect(tree.map((n) => n.id)).toEqual([1, 4, 7, 6, 5]);
  });

  it("無効な組織も is_active を保ったまま含める", () => {
    const tree = buildOrgTree([unit(1, null), unit(2, 1, 0, false)]);
    expect(tree.find((n) => n.id === 2)?.is_active).toBe(false);
    expect(tree.find((n) => n.id === 1)?.is_active).toBe(true);
  });

  it("親が一覧に無い組織は欠けずに根として出す", () => {
    const tree = buildOrgTree([unit(2, 99)]);
    expect(tree).toHaveLength(1);
    expect(tree[0]).toMatchObject({ id: 2, depth: 0 });
  });

  it("空の一覧は空の木", () => {
    expect(buildOrgTree([])).toEqual([]);
  });
});

describe("descendantIds", () => {
  it("自分と配下のすべてを返す（兄弟は含めない）", () => {
    const units = [unit(1, null), unit(2, 1), unit(3, 2), unit(4, 1)];
    expect([...descendantIds(units, 2)].sort()).toEqual([2, 3]);
    expect([...descendantIds(units, 4)]).toEqual([4]);
  });
});

describe("groupPermissionsByDomain", () => {
  it("ドメインごとにまとめ、ドメインの順は最初に現れた順、中の順は保つ", () => {
    const groups = groupPermissionsByDomain([
      perm("contract:view", "contract"),
      perm("payroll:view", "payroll"),
      perm("contract:edit", "contract"),
    ]);
    expect(groups.map((g) => g.domain)).toEqual(["contract", "payroll"]);
    expect(groups[0].items.map((i) => i.code)).toEqual(["contract:view", "contract:edit"]);
    expect(groups[1].items).toHaveLength(1);
  });

  it("空のカタログは空", () => {
    expect(groupPermissionsByDomain([])).toEqual([]);
  });
});

describe("findNewlyGrantedSensitive", () => {
  const catalog = [
    perm("contract:view", "contract"),
    perm("payroll:slip.view", "payroll", { is_sensitive: true }),
    perm("system:user.edit", "system", { is_sensitive: true }),
  ];

  it("今まで付いていない機微な権限だけを返す", () => {
    const before = [{ permission_code: "contract:view", scope: "GLOBAL" as const }];
    const after = [
      { permission_code: "contract:view", scope: "GLOBAL" as const },
      { permission_code: "payroll:slip.view", scope: "GLOBAL" as const },
    ];
    expect(findNewlyGrantedSensitive(before, after, catalog)).toEqual(["payroll:slip.view"]);
  });

  it("既に付いていた機微な権限でも、範囲を広げたら返す(レビュー指摘で仕様を変更)", () => {
    const before = [{ permission_code: "payroll:slip.view", scope: "UNIT" as const }];
    const after = [{ permission_code: "payroll:slip.view", scope: "GLOBAL" as const }];
    expect(findNewlyGrantedSensitive(before, after, catalog)).toEqual(["payroll:slip.view"]);
  });

  it("機微でない権限を新たに付けても返さない", () => {
    expect(findNewlyGrantedSensitive([], [{ permission_code: "contract:view", scope: "GLOBAL" }], catalog)).toEqual([]);
  });

  it("外したものは返さない", () => {
    const before = [{ permission_code: "system:user.edit", scope: "GLOBAL" as const }];
    expect(findNewlyGrantedSensitive(before, [], catalog)).toEqual([]);
  });
});

describe("draft の往復", () => {
  const catalog = [
    perm("contract:view", "contract", { is_stamped: true, allowed_scopes: ["PRIVATE", "UNIT", "HIERARCHICAL", "GLOBAL"] }),
    perm("payroll:slip.view", "payroll"),
  ];

  it("サーバーの権限を下書きにし、付いていない権限は初期範囲で未付与にする", () => {
    const draft = draftFromServer(catalog, [{ permission_code: "contract:view", scope: "HIERARCHICAL" }]);
    expect(draft["contract:view"]).toEqual({ granted: true, scope: "HIERARCHICAL" });
    expect(draft["payroll:slip.view"]).toEqual({ granted: false, scope: "GLOBAL" });
  });

  it("保存用の一覧は付与済みだけをカタログの順で返す", () => {
    const draft = draftFromServer(catalog, []);
    draft["payroll:slip.view"] = { granted: true, scope: "GLOBAL" };
    draft["contract:view"] = { granted: true, scope: "UNIT" };
    expect(draftToPayload(catalog, draft)).toEqual([
      { permission_code: "contract:view", scope: "UNIT" },
      { permission_code: "payroll:slip.view", scope: "GLOBAL" },
    ]);
  });

  it("isSameGrants は順序を問わず、範囲の違いを区別する", () => {
    const a = [
      { permission_code: "a", scope: "UNIT" as const },
      { permission_code: "b", scope: "GLOBAL" as const },
    ];
    const reordered = [a[1], a[0]];
    expect(isSameGrants(a, reordered)).toBe(true);
    expect(isSameGrants(a, [a[0], { permission_code: "b", scope: "UNIT" }])).toBe(false);
    expect(isSameGrants(a, [a[0]])).toBe(false);
  });
});

describe("findNewlyGrantedSensitive(範囲を広げた場合)", () => {
  const catalog = [perm("contract.price:view", "contract", { is_sensitive: true })];
  it("機微な権限の範囲を広げたら、確認の対象にする", () => {
    const before = [{ permission_code: "contract.price:view", scope: "PRIVATE" as const }];
    const after = [{ permission_code: "contract.price:view", scope: "GLOBAL" as const }];
    expect(findNewlyGrantedSensitive(before, after, catalog)).toEqual(["contract.price:view"]);
  });
  it("範囲を狭めた・変えないときは、確認しない", () => {
    const before = [{ permission_code: "contract.price:view", scope: "GLOBAL" as const }];
    expect(findNewlyGrantedSensitive(before, [{ permission_code: "contract.price:view", scope: "HIERARCHICAL" as const }], catalog)).toEqual([]);
    expect(findNewlyGrantedSensitive(before, before, catalog)).toEqual([]);
  });
});

describe("disabledOrgIds", () => {
  it("無効な組織の配下も、割り当てできない組織として扱う", () => {
    const units = [unit(1, null), unit(2, 1, 0, false), unit(3, 2), unit(31, 3), unit(5, 1)];
    expect([...disabledOrgIds(units)].sort((a, b) => a - b)).toEqual([2, 3, 31]);
  });
});

describe("変更履歴の表示名", () => {
  it("サーバが記録する対象・操作の種類は、すべて日本語で出す", () => {
    // サーバ側(log_change / org_change_log_repo::insert)で使っている種類。増やしたら両方を直す
    for (const k of ["org_unit", "role", "role_permission", "assignment", "owner_transfer", "auto_import_receiver"]) {
      expect(CHANGE_TARGET_LABEL[k], k).toBeTruthy();
    }
    for (const a of ["create", "update", "move", "delete", "transfer"]) {
      expect(CHANGE_ACTION_LABEL[a], a).toBeTruthy();
    }
  });
  it("自動取込の受け皿は番号を出さない", () => {
    expect(showChangeTargetId({ target_kind: "auto_import_receiver", target_id: 0 })).toBe(false);
    expect(showChangeTargetId({ target_kind: "org_unit", target_id: 2 })).toBe(true);
    expect(showChangeTargetId({ target_kind: "owner_transfer", target_id: null })).toBe(false);
  });
});
