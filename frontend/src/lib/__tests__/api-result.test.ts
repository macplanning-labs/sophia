import { describe, it, expect } from "vitest";
import { isApiOk } from "@/lib/api-result";

describe("isApiOk", () => {
  it("success と ok のどちらで返っても成功と判断する", () => {
    expect(isApiOk({ success: true })).toBe(true);
    expect(isApiOk({ ok: true })).toBe(true);
  });
  it("どちらも true でなければ失敗", () => {
    expect(isApiOk({ ok: false, success: undefined })).toBe(false);
    expect(isApiOk({ success: false })).toBe(false);
    expect(isApiOk({})).toBe(false);
    expect(isApiOk(null)).toBe(false);
  });
});
