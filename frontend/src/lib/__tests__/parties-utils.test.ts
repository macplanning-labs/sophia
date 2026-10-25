/**
 * parties-utils.test.ts
 *
 * 取引先画面の共通関数テスト
 */

import { describe, it, expect } from "vitest";
import { buildClientCreatePayload, formatBillingUnit, formatOwner, partiesHref, validateClientOwner, withPartyType } from "@/lib/parties-utils";

describe("formatBillingUnit", () => {
  it("PROJECT を '案件ごと' に変換", () => {
    expect(formatBillingUnit("PROJECT")).toBe("案件ごと");
  });

  it("CLIENT を '取引先まとめ' に変換", () => {
    expect(formatBillingUnit("CLIENT")).toBe("取引先まとめ");
  });

  it("undefined は空文字列を返す", () => {
    expect(formatBillingUnit(undefined)).toBe("");
  });

  it("空文字列は空文字列を返す", () => {
    expect(formatBillingUnit("")).toBe("");
  });

  it("未知の値はそのまま返す", () => {
    expect(formatBillingUnit("UNKNOWN")).toBe("UNKNOWN");
  });
});

describe("validateClientOwner", () => {
  it("組織も担当者も未選択ならエラー", () => {
    expect(validateClientOwner("", "")).toBe("担当の組織と担当者を選んでください");
  });

  it("組織だけ未選択ならエラー", () => {
    expect(validateClientOwner("", "5")).toBe("担当の組織を選んでください");
  });

  it("担当者だけ未選択ならエラー", () => {
    expect(validateClientOwner("3", "")).toBe("担当者を選んでください");
  });

  it("両方選んでいれば undefined", () => {
    expect(validateClientOwner("3", "5")).toBeUndefined();
  });
});

describe("buildClientCreatePayload", () => {
  const form = { name: " 株式会社サンプル ", contact_person: "", email: "a@example.com", billing_unit: "CLIENT" };

  it("担当を数値で、空欄は null で送る", () => {
    expect(buildClientCreatePayload(form, "3", "5")).toEqual({
      name: "株式会社サンプル",
      contact_person: null,
      email: "a@example.com",
      billing_unit: "CLIENT",
      owner_org_unit_id: 3,
      owner_employee_id: 5,
    });
  });
});

describe("formatOwner", () => {
  it("組織と担当者を / でつなぐ", () => {
    expect(formatOwner("営業1課", "山田 太郎")).toBe("営業1課 / 山田 太郎");
  });
  it("片方だけならその名前", () => {
    expect(formatOwner("営業1課", null)).toBe("営業1課");
    expect(formatOwner(undefined, "山田 太郎")).toBe("山田 太郎");
  });
  it("どちらも無ければ 未設定", () => {
    expect(formatOwner(null, null)).toBe("未設定");
    expect(formatOwner("", "  ")).toBe("未設定");
  });
});

describe("partiesHref / withPartyType", () => {
  it("取引先画面を指す", () => {
    expect(partiesHref("client")).toBe("/parties?type=client");
    expect(partiesHref("partner", "0000000007")).toBe("/parties?type=partner&open=0000000007");
    expect(partiesHref("client", 3)).toBe("/parties?type=client&open=3");
  });
  it("種別の切り替えは、ほかのパラメータを残し、?open= を外す", () => {
    expect(withPartyType(new URLSearchParams("tab=parties&type=client&open=3"), "partner")).toBe("?tab=parties&type=partner");
  });
});
