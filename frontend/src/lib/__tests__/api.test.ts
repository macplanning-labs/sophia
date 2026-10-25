import { afterEach, describe, expect, it, vi } from "vitest";
import { apiDelete, apiPost, approveInvoice, rejectInvoice, revokeApiKey, sendInvoiceMail } from "@/lib/api";
import { SessionExpiredError, subscribeSessionExpired } from "@/lib/session-expired";

function mockFetchOnce(response: Partial<Response> & { jsonBody?: unknown }) {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: response.ok ?? true,
    status: response.status ?? 200,
    statusText: response.statusText ?? "OK",
    json: async () => response.jsonBody ?? {},
    text: async () => JSON.stringify(response.jsonBody ?? {}),
    clone() { return this; },
  } as Response);
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("invoice approval workflow API (誤送信防止ガード・承認フローの回帰テスト)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("approveInvoice POSTs to /api/v1/invoices/{id}/approve", async () => {
    const fetchMock = mockFetchOnce({ jsonBody: { success: true } });

    const result = await approveInvoice("42");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/invoices/42/approve",
      expect.objectContaining({ method: "POST", credentials: "include" })
    );
    expect(result).toEqual({ success: true });
  });

  it("rejectInvoice POSTs to /api/v1/invoices/{id}/reject", async () => {
    const fetchMock = mockFetchOnce({ jsonBody: { success: true } });

    await rejectInvoice("42");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/invoices/42/reject",
      expect.objectContaining({ method: "POST" })
    );
  });

  it("sendInvoiceMail POSTs the edited subject/body to /api/v1/invoices/{id}/send", async () => {
    const fetchMock = mockFetchOnce({ jsonBody: { success: true } });

    await sendInvoiceMail("42", { subject: "件名", body: "本文" });

    const [, init] = fetchMock.mock.calls[0];
    expect(JSON.parse(init.body as string)).toEqual({ subject: "件名", body: "本文" });
  });

  it("surfaces the server-provided error message when the request fails", async () => {
    mockFetchOnce({
      ok: false,
      status: 409,
      statusText: "Conflict",
      jsonBody: { success: false, error: "承認待ちの請求書のみ承認できます" },
    });

    await expect(approveInvoice("42")).rejects.toThrow("承認待ちの請求書のみ承認できます");
  });

  it("throws SessionExpiredError and notifies the session-expired modal on 401", async () => {
    mockFetchOnce({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      jsonBody: { error: "SESSION_EXPIRED", login_url: "/login" },
    });

    const notified = vi.fn();
    const unsubscribe = subscribeSessionExpired(notified);

    await expect(approveInvoice("42")).rejects.toBeInstanceOf(SessionExpiredError);
    expect(notified).toHaveBeenCalledWith("/login");

    unsubscribe();
  });

  it("routes portal users to /portal/login based on the backend-provided login_url", async () => {
    mockFetchOnce({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      jsonBody: { error: "SESSION_EXPIRED", login_url: "/portal/login" },
    });

    const notified = vi.fn();
    const unsubscribe = subscribeSessionExpired(notified);

    await expect(approveInvoice("42")).rejects.toBeInstanceOf(SessionExpiredError);
    expect(notified).toHaveBeenCalledWith("/portal/login");

    unsubscribe();
  });

  it("does not notify the session-expired modal when already on the login page", async () => {
    vi.stubGlobal("location", { ...window.location, pathname: "/login" });
    mockFetchOnce({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      jsonBody: { error: "SESSION_EXPIRED", login_url: "/login" },
    });

    const notified = vi.fn();
    const unsubscribe = subscribeSessionExpired(notified);

    await expect(approveInvoice("42")).rejects.toBeInstanceOf(SessionExpiredError);
    expect(notified).not.toHaveBeenCalled();

    unsubscribe();
  });

  it("does not notify the session-expired modal on the public /token/ partner link page", async () => {
    vi.stubGlobal("location", { ...window.location, pathname: "/token/abc-123" });
    mockFetchOnce({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      jsonBody: { error: "SESSION_EXPIRED", login_url: "/login" },
    });

    const notified = vi.fn();
    const unsubscribe = subscribeSessionExpired(notified);

    await expect(approveInvoice("42")).rejects.toBeInstanceOf(SessionExpiredError);
    expect(notified).not.toHaveBeenCalled();

    unsubscribe();
  });
});

describe("本文なしの成功応答(204)を、エラーにしない(DEMO-000180)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  // 実物の Response を使う(本文なしを .json() で読むと、実際に例外になることを含めて確かめる)
  function stubFetch(status: number, body: string | null) {
    const fetchMock = vi.fn().mockImplementation(async () => new Response(body, { status }));
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  it("APIキーの失効: サーバーが 204 を返しても成功として終わる", async () => {
    const fetchMock = stubFetch(204, null);
    await expect(revokeApiKey("abc")).resolves.toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/settings/api-keys/abc/revoke",
      expect.objectContaining({ method: "POST" })
    );
  });

  it("共通の apiPost / apiDelete も、204 と 205 を undefined で返す", async () => {
    stubFetch(204, null);
    await expect(apiDelete("/api/v1/x")).resolves.toBeUndefined();
    stubFetch(205, null);
    await expect(apiPost("/api/v1/x", {})).resolves.toBeUndefined();
  });

  it("本文がある成功応答は、従来どおり JSON として返す", async () => {
    stubFetch(200, JSON.stringify({ success: true }));
    await expect(apiPost("/api/v1/x", {})).resolves.toEqual({ success: true });
  });

  it("失敗応答(404)は、従来どおりエラーにする", async () => {
    stubFetch(404, JSON.stringify({ error: "見つかりません" }));
    await expect(revokeApiKey("missing")).rejects.toThrow("見つかりません");
  });
});
