import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import LoginPage from "@/app/login/page";

const pushMock = vi.fn();
let searchParamsValue = new URLSearchParams();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: pushMock }),
  useSearchParams: () => searchParamsValue,
}));

function renderLoginPage() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <LoginPage />
    </QueryClientProvider>
  );
}

function mockLoginResponse(body: unknown, status = 200) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      status,
      json: async () => body,
    } as Response)
  );
}

describe("LoginPage（ログインのクリティカルパス）", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    pushMock.mockClear();
    searchParamsValue = new URLSearchParams();
    cleanup();
  });

  it("renders the email and password fields and the submit button", () => {
    renderLoginPage();
    expect(screen.getByPlaceholderText("admin@example.com")).toBeDefined();
    expect(screen.getByPlaceholderText("••••••••")).toBeDefined();
    expect(screen.getByRole("button", { name: "ログイン" })).toBeDefined();
  });

  it("redirects ADMIN/staff users to the dashboard on success", async () => {
    mockLoginResponse({ success: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
  });

  it("redirects PARTNER/ENGINEER users to the portal instead of the staff dashboard", async () => {
    mockLoginResponse({ success: true, role: "PARTNER" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "partner@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/portal/timesheet-entry"));
  });

  it("routes to MFA verification when mfa_required is set, without granting access yet", async () => {
    mockLoginResponse({ success: true, mfa_required: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/mfa"));
    expect(pushMock).not.toHaveBeenCalledWith("/");
  });

  it("routes to security settings when mfa_setup_required is set", async () => {
    mockLoginResponse({ success: true, mfa_setup_required: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/settings/security"));
    expect(pushMock).not.toHaveBeenCalledWith("/");
  });

  it("redirects to the redirect param destination when present and safe (session-expired modal flow)", async () => {
    searchParamsValue = new URLSearchParams({ redirect: "/settlement?month=2026-08" });
    mockLoginResponse({ success: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/settlement?month=2026-08"));
  });

  it("ignores an unsafe redirect param (open-redirect attempt) and falls back to the default", async () => {
    searchParamsValue = new URLSearchParams({ redirect: "//evil.example.com" });
    mockLoginResponse({ success: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/"));
    expect(pushMock).not.toHaveBeenCalledWith("//evil.example.com");
  });

  it("prioritizes the MFA verification route over the redirect param", async () => {
    searchParamsValue = new URLSearchParams({ redirect: "/settlement" });
    mockLoginResponse({ success: true, mfa_required: true, role: "ADMIN" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "admin" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    await waitFor(() => expect(pushMock).toHaveBeenCalledWith("/mfa"));
    expect(pushMock).not.toHaveBeenCalledWith("/settlement");
  });

  it("shows the server error message and does not navigate on failed login", async () => {
    mockLoginResponse({ success: false, error: "メールアドレスまたはパスワードが違います" });
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "wrong" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    expect(await screen.findByText("メールアドレスまたはパスワードが違います")).toBeDefined();
    expect(pushMock).not.toHaveBeenCalled();
  });

  it("shows a Japanese lockout message with wait time for the account_locked (429) response", async () => {
    mockLoginResponse({ error: "account_locked", retry_after_seconds: 900 }, 429);
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "wrong" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    expect(
      await screen.findByText("試行回数が上限に達しました。15分ほど時間をおいて再度お試しください")
    ).toBeDefined();
    expect(pushMock).not.toHaveBeenCalled();
  });

  it("falls back to a generic lockout message when retry_after_seconds is missing", async () => {
    mockLoginResponse({ error: "account_locked" }, 429);
    renderLoginPage();

    fireEvent.change(screen.getByPlaceholderText("admin@example.com"), {
      target: { value: "admin@example.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("••••••••"), {
      target: { value: "wrong" },
    });
    fireEvent.click(screen.getByRole("button", { name: "ログイン" }));

    expect(
      await screen.findByText("試行回数が上限に達しました。しばらくしてから再度お試しください")
    ).toBeDefined();
    expect(pushMock).not.toHaveBeenCalled();
  });
});
