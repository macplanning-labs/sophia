import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import LoginPage from "@/app/login/page";
import { markJustLoggedOut, SSO_DENIED_MESSAGE } from "@/lib/sso-login";

// 社内アカウント(SSO)でのログイン(SOSSO F1 / DEMO-000237)のテスト。
// 流れ: 画面を開く → SSO が有効なら、すでに Cloudflare Access を通っているブラウザなら静かに(自動で 1 回だけ)ログイン。
//       まだ通っていない(401)なら、エラーにせず、ボタンを出す → ボタンは Access で保護した入口へ「画面ごと」移動する。
// 確かめること: 無効なサーバーでは出さない・試さない / 自動は 1 回だけ・失敗しても勝手に試し直さない(無限ループにならない) /
//   ログアウト直後と直近の自動試行の直後は自動で試さない / 成功したら遷移 / 入口から戻ってきた失敗(?sso_error=1)の表示。

const pushMock = vi.fn();
const assignMock = vi.fn();
let searchParamsValue = new URLSearchParams();
const realLocation = window.location;

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: pushMock }),
  useSearchParams: () => searchParamsValue,
}));

type SsoReply = { status: number; body?: unknown };

/** SSO の問い合わせ(status)と、ログイン(login)の応答を指定して、fetch を差し替える。呼び出しの記録を返す。 */
function stubFetch(opts: { enabled: boolean; login?: SsoReply; loginDelayMs?: number }) {
  const calls: { url: string; method: string }[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      const u = String(url);
      calls.push({ url: u, method: init?.method ?? "GET" });
      if (u.endsWith("/auth/sso/status")) {
        return { status: 200, ok: true, json: async () => ({ enabled: opts.enabled }) } as Response;
      }
      if (u.endsWith("/auth/sso/login")) {
        if (opts.loginDelayMs) await new Promise((r) => setTimeout(r, opts.loginDelayMs));
        const r = opts.login ?? { status: 401 };
        return { status: r.status, ok: r.status < 400, json: async () => r.body ?? {} } as Response;
      }
      return { status: 200, ok: true, json: async () => ({}) } as Response;
    })
  );
  return calls;
}

function renderLogin() {
  const client = new QueryClient();
  const clearSpy = vi.spyOn(client, "clear");
  render(
    <QueryClientProvider client={client}>
      <LoginPage />
    </QueryClientProvider>
  );
  return { clearSpy };
}

const ssoPosts = (calls: { url: string; method: string }[]) =>
  calls.filter((c) => c.url.endsWith("/auth/sso/login") && c.method === "POST");

const SSO_BUTTON = "🏢 社内アカウントでログイン";
const RETRY_BUTTON = "🏢 社内アカウントでもう一度試す";
const OK = { status: 200, body: { success: true, role: "ADMIN" } };

describe("LoginPage — 社内アカウント(SSO)", () => {
  beforeEach(() => {
    window.sessionStorage.clear();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...realLocation, assign: assignMock },
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    pushMock.mockClear();
    assignMock.mockClear();
    searchParamsValue = new URLSearchParams();
    window.sessionStorage.clear();
    Object.defineProperty(window, "location", { configurable: true, value: realLocation });
    cleanup();
  });

  it("SSO が無効なサーバーでは、ボタンを出さず、ログインも試さない(いまの画面のまま)", async () => {
    const calls = stubFetch({ enabled: false });
    renderLogin();
    await waitFor(() => expect(calls.some((c) => c.url.endsWith("/auth/sso/status"))).toBe(true));
    expect(screen.queryByRole("button", { name: SSO_BUTTON })).toBeNull();
    expect(ssoPosts(calls)).toHaveLength(0);
    expect(screen.getByPlaceholderText("admin@example.com")).toBeDefined();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("すでに Access を通っているブラウザなら、自動で 1 回だけ試して、前のキャッシュを消して遷移する", async () => {
    const calls = stubFetch({ enabled: true, login: OK });
    const { clearSpy } = renderLogin();
    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
    expect(ssoPosts(calls)).toHaveLength(1);
    expect(clearSpy).toHaveBeenCalled();
    expect(assignMock).not.toHaveBeenCalled();
  });

  it("成功したとき、安全な redirect 指定があればそこへ、外部サイトなら無視して / へ", async () => {
    searchParamsValue = new URLSearchParams("redirect=/payroll");
    stubFetch({ enabled: true, login: OK });
    renderLogin();
    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/payroll"));
    cleanup();
    pushMock.mockClear();
    window.sessionStorage.clear();

    searchParamsValue = new URLSearchParams("redirect=//evil.example.com");
    stubFetch({ enabled: true, login: { status: 200, body: { success: true, role: "EMPLOYEE" } } });
    renderLogin();
    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
    expect(pushMock).not.toHaveBeenCalledWith("//evil.example.com");
  });

  it("確認している間も、メール・パスワードのフォームは使える(確認中は、社内アカウントのボタンを出さない)", async () => {
    stubFetch({ enabled: true, login: OK, loginDelayMs: 150 });
    renderLogin();
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("社内アカウントを確認しています"));
    expect(screen.getByPlaceholderText("admin@example.com")).toBeDefined();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
    expect(screen.queryByRole("button", { name: SSO_BUTTON })).toBeNull();
    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
  });

  it("まだ Access を通っていない(401)のは、エラーにしない。ボタンを出し、自動では二度と試さない", async () => {
    const calls = stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    const button = await screen.findByRole("button", { name: SSO_BUTTON });
    expect(screen.queryByText(SSO_DENIED_MESSAGE)).toBeNull(); // 初めての人に、失敗の表示を出さない
    expect(screen.queryByRole("button", { name: RETRY_BUTTON })).toBeNull();
    expect(pushMock).not.toHaveBeenCalled();
    // 少し待っても、勝手に試し直さない(無限ループにならない)
    await new Promise((r) => setTimeout(r, 200));
    expect(ssoPosts(calls)).toHaveLength(1);
    expect(button).toBeDefined();
  });

  it("ボタンは、Access で保護した入口へ画面ごと移動する(fetch ではない)", async () => {
    const calls = stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    fireEvent.click(await screen.findByRole("button", { name: SSO_BUTTON }));
    expect(assignMock).toHaveBeenCalledWith("/api/v1/auth/sso/start");
    expect(ssoPosts(calls)).toHaveLength(1); // 自動の 1 回だけ。ボタンで POST は増えない
  });

  it("ボタンで移動するとき、安全な redirect だけを引き継ぐ", async () => {
    searchParamsValue = new URLSearchParams("redirect=/payroll");
    stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    fireEvent.click(await screen.findByRole("button", { name: SSO_BUTTON }));
    expect(assignMock).toHaveBeenCalledWith("/api/v1/auth/sso/start?redirect=%2Fpayroll");
    cleanup();
    assignMock.mockClear();
    window.sessionStorage.clear();

    searchParamsValue = new URLSearchParams("redirect=//evil.example.com");
    stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    fireEvent.click(await screen.findByRole("button", { name: SSO_BUTTON }));
    expect(assignMock).toHaveBeenCalledWith("/api/v1/auth/sso/start");
  });

  it("Access は通ったが入れない(403)とき、理由を出さずに従来のフォームへ。「もう一度」が出て、自動では二度と試さない", async () => {
    const calls = stubFetch({ enabled: true, login: { status: 403 } });
    renderLogin();
    await waitFor(() => expect(screen.getByText(SSO_DENIED_MESSAGE)).toBeDefined());
    expect(pushMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: RETRY_BUTTON })).toBeDefined();
    expect(screen.queryByRole("button", { name: SSO_BUTTON })).toBeNull();
    await new Promise((r) => setTimeout(r, 200));
    expect(ssoPosts(calls)).toHaveLength(1);
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("「もう一度試す」も、入口へ移動する", async () => {
    stubFetch({ enabled: true, login: { status: 403 } });
    renderLogin();
    await waitFor(() => expect(screen.getByText(SSO_DENIED_MESSAGE)).toBeDefined());
    fireEvent.click(screen.getByRole("button", { name: RETRY_BUTTON }));
    expect(assignMock).toHaveBeenCalledWith("/api/v1/auth/sso/start");
  });

  it("入口から入れずに戻ってきた(?sso_error=1)とき、失敗の表示と「もう一度」を出し、自動では試さない", async () => {
    searchParamsValue = new URLSearchParams("sso_error=1");
    const calls = stubFetch({ enabled: true, login: OK });
    renderLogin();
    await waitFor(() => expect(screen.getByText(SSO_DENIED_MESSAGE)).toBeDefined());
    expect(screen.getByRole("button", { name: RETRY_BUTTON })).toBeDefined();
    await new Promise((r) => setTimeout(r, 100));
    expect(ssoPosts(calls)).toHaveLength(0); // 自動で試さない(入口 → 失敗 → 自動 → 入口、の往復にならない)
    expect(pushMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("?sso_error=1 でも、SSO が無効なサーバーでは、何も出さない", async () => {
    searchParamsValue = new URLSearchParams("sso_error=1");
    stubFetch({ enabled: false });
    renderLogin();
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByText(SSO_DENIED_MESSAGE)).toBeNull();
    expect(screen.queryByRole("button", { name: RETRY_BUTTON })).toBeNull();
  });

  it("ログアウトした直後は、自動では試さない。ボタンは出る", async () => {
    markJustLoggedOut();
    const calls = stubFetch({ enabled: true, login: OK });
    renderLogin();
    await screen.findByRole("button", { name: SSO_BUTTON });
    expect(ssoPosts(calls)).toHaveLength(0);
    expect(pushMock).not.toHaveBeenCalled();
  });

  it("ログアウトの印は 1 回だけ有効(次に開いたときは、また自動で試す)", async () => {
    markJustLoggedOut();
    stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    await screen.findByRole("button", { name: SSO_BUTTON });
    cleanup();
    window.sessionStorage.removeItem("sophia_sso_auto_attempt_at");
    const calls = stubFetch({ enabled: true, login: { status: 401 } });
    renderLogin();
    await waitFor(() => expect(ssoPosts(calls)).toHaveLength(1));
  });

  it("直近 60 秒以内に自動で試していたら、自動では試さない(ログインと画面の往復を止める)", async () => {
    window.sessionStorage.setItem("sophia_sso_auto_attempt_at", String(Date.now() - 5_000));
    const calls = stubFetch({ enabled: true, login: OK });
    renderLogin();
    await screen.findByRole("button", { name: SSO_BUTTON });
    expect(ssoPosts(calls)).toHaveLength(0);
    expect(pushMock).not.toHaveBeenCalled();
  });

  it("60 秒より前の自動試行は、数えない(また自動で試す)", async () => {
    window.sessionStorage.setItem("sophia_sso_auto_attempt_at", String(Date.now() - 120_000));
    stubFetch({ enabled: true, login: OK });
    renderLogin();
    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
  });

  it("自動で試したら 404(途中で無効になった)なら、SSO の表示を出さない", async () => {
    stubFetch({ enabled: true, login: { status: 404 } });
    renderLogin();
    await waitFor(() => expect(screen.queryByRole("button", { name: SSO_BUTTON })).toBeNull());
    expect(screen.queryByText(SSO_DENIED_MESSAGE)).toBeNull();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("サーバーに接続できないとき(通信エラー)は、従来のフォームが使え、「もう一度」が出る", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockImplementation(async (url: string) => {
        if (String(url).endsWith("/auth/sso/status")) {
          return { status: 200, ok: true, json: async () => ({ enabled: true }) } as Response;
        }
        throw new TypeError("network");
      })
    );
    renderLogin();
    await waitFor(() => expect(screen.getByText("サーバーに接続できません")).toBeDefined());
    expect(screen.getByRole("button", { name: RETRY_BUTTON })).toBeDefined();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("状態の問い合わせが失敗したら、SSO は無いものとして扱う(従来のログインだけ)", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("network")));
    renderLogin();
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole("button", { name: SSO_BUTTON })).toBeNull();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });
});
