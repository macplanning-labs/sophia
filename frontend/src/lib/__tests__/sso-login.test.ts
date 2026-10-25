import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AUTO_ATTEMPT_INTERVAL_MS,
  canAutoAttempt,
  consumeJustLoggedOut,
  fetchSsoEnabled,
  markJustLoggedOut,
  recordAutoAttempt,
  SSO_DENIED_MESSAGE,
  ssoEntryUrl,
  trySsoLogin,
} from "@/lib/sso-login";

describe("sso-login(社内アカウントでのログインの部品)", () => {
  beforeEach(() => window.sessionStorage.clear());
  afterEach(() => {
    vi.unstubAllGlobals();
    window.sessionStorage.clear();
  });

  it("ログアウトの印は、1 回だけ有効", () => {
    expect(consumeJustLoggedOut()).toBe(false);
    markJustLoggedOut();
    expect(consumeJustLoggedOut()).toBe(true);
    expect(consumeJustLoggedOut()).toBe(false);
  });

  it("自動で試すのは、直近に試していないときだけ", () => {
    const t0 = 1_000_000;
    expect(canAutoAttempt(t0)).toBe(true);
    recordAutoAttempt(t0);
    expect(canAutoAttempt(t0 + 1_000)).toBe(false);
    expect(canAutoAttempt(t0 + AUTO_ATTEMPT_INTERVAL_MS - 1)).toBe(false);
    expect(canAutoAttempt(t0 + AUTO_ATTEMPT_INTERVAL_MS)).toBe(true);
  });

  it("保存領域が使えなくても、例外を出さずに動き、自動では試さない(往復を防ぐ)", () => {
    vi.spyOn(window, "sessionStorage", "get").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => markJustLoggedOut()).not.toThrow();
    expect(consumeJustLoggedOut()).toBe(false);
    expect(canAutoAttempt()).toBe(false);
    expect(() => recordAutoAttempt()).not.toThrow();
    vi.restoreAllMocks();
  });

  it("fetchSsoEnabled は、enabled が true のときだけ true(それ以外・失敗は false)", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => ({ enabled: true }) }));
    expect(await fetchSsoEnabled()).toBe(true);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => ({ enabled: "true" }) }));
    expect(await fetchSsoEnabled()).toBe(false);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, json: async () => ({ enabled: true }) }));
    expect(await fetchSsoEnabled()).toBe(false);
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("x")));
    expect(await fetchSsoEnabled()).toBe(false);
  });

  it("trySsoLogin の結果の分け方(401 は『Access をまだ通っていない』でエラーではない)", async () => {
    const reply = (status: number, body: unknown = {}) =>
      vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ status, json: async () => body }));
    reply(200, { success: true, role: "ADMIN" });
    expect(await trySsoLogin()).toEqual({ status: "loggedIn", role: "ADMIN" });
    reply(404);
    expect(await trySsoLogin()).toEqual({ status: "disabled" });
    reply(401);
    expect(await trySsoLogin()).toEqual({ status: "failed", reason: "unauthorized", message: SSO_DENIED_MESSAGE });
    reply(403, { error: "理由を出してはいけない" });
    expect(await trySsoLogin()).toEqual({ status: "failed", reason: "denied", message: SSO_DENIED_MESSAGE });
    reply(500, { error: "ログイン処理に失敗しました" });
    expect(await trySsoLogin()).toEqual({ status: "failed", reason: "error", message: "ログイン処理に失敗しました" });
    // 200 でも success でなければ失敗
    reply(200, { success: false });
    expect((await trySsoLogin()).status).toBe("failed");
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("x")));
    expect(await trySsoLogin()).toEqual({ status: "failed", reason: "error", message: "サーバーに接続できません" });
  });

  it("ssoEntryUrl は、サイト内の相対パスだけを redirect に引き継ぐ", () => {
    expect(ssoEntryUrl(null)).toBe("/api/v1/auth/sso/start");
    expect(ssoEntryUrl(undefined)).toBe("/api/v1/auth/sso/start");
    expect(ssoEntryUrl("/payroll")).toBe("/api/v1/auth/sso/start?redirect=%2Fpayroll");
    expect(ssoEntryUrl("/orders?tab=a&x=1")).toBe("/api/v1/auth/sso/start?redirect=%2Forders%3Ftab%3Da%26x%3D1");
    for (const bad of ["//evil.example.com", "https://evil.example.com", "evil", ""]) {
      expect(ssoEntryUrl(bad), bad).toBe("/api/v1/auth/sso/start");
    }
  });
});
