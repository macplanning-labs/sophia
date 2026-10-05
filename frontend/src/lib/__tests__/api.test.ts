import { afterEach, describe, expect, it, vi } from "vitest";
import { approveInvoice, rejectInvoice, sendInvoiceMail } from "@/lib/api";
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
