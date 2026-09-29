/**
 * sidebar.test.ts
 *
 * サイドバーのナビ構成（請求書への常設導線）の検証。
 */

import { describe, it, expect } from "vitest";
import { buildNavGroups } from "@/components/layout/sidebar";

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
