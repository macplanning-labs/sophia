import { describe, expect, it } from "vitest";
import { formatExpiry } from "../api-key-expiry";

const NOW = Date.parse("2026-10-09T00:00:00Z");

describe("formatExpiry", () => {
  it("未設定(null/undefined/空)は無期限", () => {
    expect(formatExpiry(null, NOW)).toBe("無期限");
    expect(formatExpiry(undefined, NOW)).toBe("無期限");
    expect(formatExpiry("", NOW)).toBe("無期限");
  });

  it("未来の期限は「期限: 日付」", () => {
    expect(formatExpiry("2027-01-01T00:00:00Z", NOW)).toMatch(/^期限: /);
  });

  it("過ぎた期限は「期限切れ」", () => {
    expect(formatExpiry("2026-10-01T00:00:00Z", NOW)).toMatch(/^期限切れ \(/);
  });

  it("ちょうどの時刻は期限切れ(サーバーと同じく、期限の時刻から無効)", () => {
    expect(formatExpiry("2026-10-09T00:00:00Z", NOW)).toMatch(/^期限切れ/);
  });

  it("不正な日付は不明", () => {
    expect(formatExpiry("not-a-date", NOW)).toBe("期限: 不明");
  });
});
