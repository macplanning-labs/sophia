/**
 * parties-utils.test.ts
 *
 * 取引先画面の共通関数テスト
 */

import { describe, it, expect } from "vitest";
import { formatBillingUnit } from "@/lib/parties-utils";

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
