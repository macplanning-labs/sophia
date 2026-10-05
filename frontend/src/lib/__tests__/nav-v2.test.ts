/**
 * nav-v2.test.ts
 *
 * 新UI ナビゲーション設定の検証。
 * 【決定】§1-6: ready・adminOnly・順序
 */

import { describe, it, expect } from "vitest";
import { NAV_V2, visibleNavV2 } from "@/lib/nav-v2";

describe("NAV_V2", () => {
  it("定義された項目が8件", () => {
    expect(NAV_V2).toHaveLength(8);
  });

  it("すべての項目が必須フィールドを持つ", () => {
    NAV_V2.forEach((item) => {
      expect(item).toHaveProperty("key");
      expect(item).toHaveProperty("label");
      expect(item).toHaveProperty("href");
      expect(item).toHaveProperty("icon");
      expect(item).toHaveProperty("adminOnly");
      expect(item).toHaveProperty("ready");
    });
  });

  it("キーが一意", () => {
    const keys = NAV_V2.map((item) => item.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("パスが一意", () => {
    const hrefs = NAV_V2.map((item) => item.href);
    expect(new Set(hrefs).size).toBe(hrefs.length);
  });

  it("順序が設計書通り（§1-6 テーブル）", () => {
    const keys = NAV_V2.map((item) => item.key);
    expect(keys).toEqual([
      "home",
      "parties",
      "members",
      "assignments",
      "timesheet-matching",
      "billing",
      "mail",
      "status",
    ]);
  });

  it("ホーム（/home）だけが全員表示（adminOnly: false）", () => {
    const home = NAV_V2.find((item) => item.key === "home");
    expect(home?.adminOnly).toBe(false);
  });

  it("それ以外はすべて管理者限定（adminOnly: true）", () => {
    const others = NAV_V2.filter((item) => item.key !== "home");
    others.forEach((item) => {
      expect(item.adminOnly).toBe(true);
    });
  });

  it("parties / members / assignments / billing / timesheet-matching だけが ready: true（画面が出来たもの）", () => {
    const partiesItem = NAV_V2.find((item) => item.key === "parties");
    const timesheetItem = NAV_V2.find((item) => item.key === "timesheet-matching");
    expect(partiesItem?.ready).toBe(true);
    expect(timesheetItem?.ready).toBe(true);
    expect(NAV_V2.find((item) => item.key === "members")?.ready).toBe(true);

    // 他はすべて ready: false
    const readyKeys = ["parties", "members", "assignments", "billing", "timesheet-matching"];
    expect(NAV_V2.find((item) => item.key === "billing")?.ready).toBe(true);
    const others = NAV_V2.filter((item) => !readyKeys.includes(item.key));
    others.forEach((item) => {
      expect(item.ready, item.key).toBe(false);
    });
  });
});

describe("visibleNavV2", () => {
  it("管理者には取引先・要員・アサイン・請求書・勤務表が出る（ready: true の5項目）", () => {
    const visible = visibleNavV2(true);
    expect(visible).toHaveLength(5);
    expect(visible.map((i) => i.key)).toEqual(["parties", "members", "assignments", "timesheet-matching", "billing"]);
  });

  it("非管理者には何も出ない（全項目が adminOnly: true）", () => {
    expect(visibleNavV2(false)).toHaveLength(0);
  });

  const items = [
    { key: "a", label: "A", href: "/a", icon: "A", adminOnly: false, ready: true },
    { key: "b", label: "B", href: "/b", icon: "B", adminOnly: true, ready: true },
    { key: "c", label: "C", href: "/c", icon: "C", adminOnly: false, ready: false },
  ];

  it("ready: true だけが出る", () => {
    expect(visibleNavV2(true, items).map((i) => i.key)).toEqual(["a", "b"]);
  });

  it("adminOnly は非管理者に出ない", () => {
    expect(visibleNavV2(false, items).map((i) => i.key)).toEqual(["a"]);
  });
});
