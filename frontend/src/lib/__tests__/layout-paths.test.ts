import { describe, it, expect } from "vitest";
import { isChromeHidden, CHROME_HIDDEN_PATHS } from "@/lib/layout-paths";

describe("isChromeHidden", () => {
  it("ログイン・MFA・ポータル・公開ページではヘッダー/サイドバーを出さない", () => {
    for (const p of ["/login", "/mfa", "/mfa/verify", "/portal", "/portal/notices", "/upload/mobile/abc", "/invite/uuid", "/token/uuid"]) {
      expect(isChromeHidden(p), p).toBe(true);
    }
  });

  it("業務画面では出す", () => {
    for (const p of ["/", "/employees", "/employees/1", "/timesheet-matching", "/settings/security"]) {
      expect(isChromeHidden(p), p).toBe(false);
    }
  });

  it("一覧は6件(増減したら画面の枠の確認が必要)", () => {
    expect(CHROME_HIDDEN_PATHS).toHaveLength(6);
  });
});
