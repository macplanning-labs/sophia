import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { IntegrationAlertBanner } from "@/components/layout/integration-alert-banner";

let mockPathname = "/status";
vi.mock("next/navigation", () => ({ usePathname: () => mockPathname }));

let mockIsAdmin = true;
vi.mock("@/lib/useCurrentUser", () => ({
  useCurrentUser: () => ({ isAdmin: mockIsAdmin, user: null, isLoading: false, isError: false }),
}));

const mockFetch = vi.fn();
vi.mock("@/lib/api", () => ({ fetchIntegrationAlert: () => mockFetch() }));

afterEach(() => {
  cleanup();
  mockPathname = "/status";
  mockIsAdmin = true;
  mockFetch.mockReset();
});

function renderBanner() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <IntegrationAlertBanner />
    </QueryClientProvider>,
  );
}

describe("IntegrationAlertBanner", () => {
  it("管理者で問題があれば、赤い帯と、外部連携への入口を出す", async () => {
    mockFetch.mockResolvedValue({ checked_at: "2026-10-12 08:45", problems: ["メール送信: 鍵がありません"] });
    renderBanner();
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("外部連携に問題があります(1件)")).toBeTruthy();
    expect(screen.getByRole("link", { name: /外部連携を開く/ }).getAttribute("href")).toBe("/settings/integrations");
  });

  it("問題が無ければ出さない", async () => {
    mockFetch.mockResolvedValue({ checked_at: "2026-10-12 08:45", problems: [] });
    renderBanner();
    await vi.waitFor(() => expect(mockFetch).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("管理者以外には出さず、問い合わせもしない", async () => {
    mockIsAdmin = false;
    mockFetch.mockResolvedValue({ checked_at: null, problems: ["x"] });
    renderBanner();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("ログイン画面など、枠を出さないページでは出さない", async () => {
    mockPathname = "/login";
    mockFetch.mockResolvedValue({ checked_at: null, problems: ["x"] });
    renderBanner();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
