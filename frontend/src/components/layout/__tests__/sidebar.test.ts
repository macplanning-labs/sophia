/**
 * sidebar.test.ts
 *
 * サイドバーのナビ構成（請求書への常設導線）の検証。
 */

import { describe, it, expect } from "vitest";
import { buildNavGroups, roleLabel } from "@/components/layout/sidebar";

const flatItems = (isAdmin: boolean) =>
  buildNavGroups(isAdmin).flatMap((group) => group.items);

describe("buildNavGroups", () => {
  it.each([true, false])("請求書（/invoices）が含まれる（isAdmin=%s）", (isAdmin) => {
    const item = flatItems(isAdmin).find((i) => i.href === "/invoices");
    expect(item?.label).toBe("請求書");
  });

  it("請求書は管理者限定ではない（月次確定と同じ一般スタッフ向け）", () => {
    const item = flatItems(false).find((i) => i.href === "/invoices");
    expect(item?.adminOnly).toBeUndefined();
  });

  it("先頭グループで月次確定の直後に請求書が並ぶ", () => {
    const first = buildNavGroups(true)[0];
    const settlement = first.items.findIndex((i) => i.href === "/settlement");
    const invoices = first.items.findIndex((i) => i.href === "/invoices");
    expect(settlement).toBeGreaterThanOrEqual(0);
    expect(invoices).toBe(settlement + 1);
  });

  it("href が重複していない", () => {
    const hrefs = flatItems(true).map((i) => i.href);
    expect(new Set(hrefs).size).toBe(hrefs.length);
  });
});

describe("roleLabel", () => {
  it("管理者は注記なし", () => {
    expect(roleLabel("ADMIN")).toEqual({ label: "管理者" });
  });

  it("担当者には管理者専用操作が出ない旨の注記が付く", () => {
    const r = roleLabel("EMPLOYEE");
    expect(r.label).toBe("担当者");
    expect(r.note).toContain("管理者専用");
  });

  it("取得前は確認中", () => {
    expect(roleLabel(undefined).label).toBe("確認中");
  });
});

describe("buildNavGroups（新UI無効時）", () => {
  it("新UI が無効な場合、buildNavGroups の結果は従来と完全一致する", () => {
    const groups = buildNavGroups(true);

    // グループ数が5（従来のまま）
    expect(groups).toHaveLength(5);

    // 第1グループ（タイトルなし）
    expect(groups[0].title).toBe("");
    expect(groups[0].items.map((i) => i.label)).toEqual([
      "ダッシュボード",
      "案件",
      "月次確定",
      "請求書",
    ]);

    // 第2グループ「契約管理」
    expect(groups[1].title).toBe("契約管理");
    expect(groups[1].items.map((i) => i.label)).toEqual([
      "発注契約",
      "受注契約",
    ]);

    // 第3グループ「稼働・タスク」
    expect(groups[2].title).toBe("稼働・タスク");
    expect(groups[2].items.map((i) => i.href)).toContain("/timesheet-matching");

    // 第4グループ「給与」
    expect(groups[3].title).toBe("給与");
    expect(groups[3].items.map((i) => i.label)).toEqual(["給与", "社員", "経費"]);

    // 第5グループ「設定」
    expect(groups[4].title).toBe("設定");
  });
});
