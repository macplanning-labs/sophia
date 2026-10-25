/**
 * sidebar.test.ts
 *
 * サイドバーの構成(DEMO-000188: 9項目に整理し、似た画面は上部のタブでまとめる)の検証。
 */

import { describe, it, expect } from "vitest";
import { buildNavGroups, roleLabel } from "@/components/layout/sidebar";

const visibleItems = (isAdmin: boolean) =>
  buildNavGroups(isAdmin).flatMap((group) => group.items.filter((i) => !i.adminOnly || isAdmin));
const byLabel = (isAdmin: boolean, label: string) => visibleItems(isAdmin).find((i) => i.label === label);

describe("buildNavGroups(管理者)", () => {
  it("9項目で、グループと並びは合意した案のとおり", () => {
    const groups = buildNavGroups(true);
    expect(groups.map((g) => [g.title, g.items.map((i) => i.label)])).toEqual([
      ["", ["ホーム", "メール"]],
      ["契約", ["案件", "契約", "要員"]],
      ["月次", ["勤務表", "月の締め"]],
      ["社員", ["社員・給与"]],
      ["", ["設定"]],
    ]);
    expect(visibleItems(true)).toHaveLength(9);
  });

  it("ボタンの行き先は、セクションの最初のタブ", () => {
    expect(Object.fromEntries(visibleItems(true).map((i) => [i.label, i.href]))).toEqual({
      "ホーム": "/home",
      "メール": "/received-emails",
      "案件": "/projects",
      "契約": "/assignments",
      "要員": "/members",
      "勤務表": "/timesheet-matching",
      "月の締め": "/status",
      "社員・給与": "/payroll",
      "設定": "/users",
    });
  });

  it("重なっていた項目(勤務表4つ、請求書2つ、月別状況と月次確定)は、サイドバーから消えている", () => {
    const labels = visibleItems(true).map((i) => i.label);
    for (const gone of [
      "ダッシュボード", "月次確定", "請求書", "請求書・支払通知", "月別状況",
      "稼働報告", "勤務表の取り込み", "稼働報告（自己申告）", "発注契約", "受注契約", "アサイン",
      "取引先", "Peppolログ", "給与", "社員", "経費", "ユーザー管理", "マスタメンテ",
    ]) {
      expect(labels, gone).not.toContain(gone);
    }
  });

  it("href が重複していない", () => {
    const hrefs = visibleItems(true).map((i) => i.href);
    expect(new Set(hrefs).size).toBe(hrefs.length);
  });

  it("セクションのボタンは、そのセクションのどのタブの画面でも選択中になる", () => {
    const closing = byLabel(true, "月の締め")?.activeWhen;
    for (const p of ["/status", "/settlement", "/billing", "/invoices/3", "/peppol"]) {
      expect(closing?.(p), p).toBe(true);
    }
    for (const p of ["/", "/timesheets", "/projects"]) {
      expect(closing?.(p), p).toBe(false);
    }
    const home = byLabel(true, "ホーム")?.activeWhen;
    expect(home?.("/")).toBe(true);
    expect(home?.("/home")).toBe(true);
    expect(home?.("/projects")).toBe(false);
  });
});

describe("buildNavGroups(一般社員)", () => {
  it("見えるタブが無い項目(要員)と、管理者専用の項目(メール)は出ない", () => {
    expect(visibleItems(false).map((i) => i.label)).toEqual([
      "ホーム", "案件", "契約", "勤務表", "月の締め", "社員・給与", "設定",
    ]);
  });

  it("ボタンの行き先は、その人に見える最初のタブ", () => {
    expect(byLabel(false, "ホーム")?.href).toBe("/");
    expect(byLabel(false, "契約")?.href).toBe("/client-contracts");
    expect(byLabel(false, "勤務表")?.href).toBe("/timesheets");
    expect(byLabel(false, "月の締め")?.href).toBe("/settlement");
    expect(byLabel(false, "設定")?.href).toBe("/settings/security");
  });

  it("見える項目が無いグループは出さない", () => {
    for (const g of buildNavGroups(false)) {
      expect(g.items.some((i) => !i.adminOnly), g.title).toBe(true);
    }
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
