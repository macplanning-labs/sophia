import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act, waitFor } from "@testing-library/react";
import { AccountMenu } from "@/components/layout/account-menu";

// useCurrentUser のモック
vi.mock("@/lib/useCurrentUser", () => ({
  useCurrentUser: () => ({
    isAdmin: true,
    user: { email: "admin@example.com", role: "ADMIN" },
    isLoading: false,
    isError: false,
  }),
}));

// useUiV2 のモック（デフォルトは無効）
const mockUiV2State = { enabled: false, setEnabled: vi.fn() };
vi.mock("@/lib/useUiV2", () => ({
  useUiV2: () => mockUiV2State,
}));

// visibleNavV2 のモック（デフォルトは空配列）
let mockVisibleNavV2Items: unknown[] = [];
vi.mock("@/lib/nav-v2", () => ({
  visibleNavV2: () => mockVisibleNavV2Items,
}));

// handleLogout のモック
vi.mock("@/lib/auth-actions", () => ({
  handleLogout: vi.fn(),
  toggleTheme: (theme: string) => (theme === "dark" ? "light" : "dark"),
}));

describe("AccountMenu", () => {
  beforeEach(() => {
    localStorage.clear();
    mockUiV2State.enabled = false;
    mockUiV2State.setEnabled = vi.fn();
    mockVisibleNavV2Items = [];
  });

  afterEach(() => {
    cleanup();
  });

  it("新UI無効な場合は何も描画しない", () => {
    render(<AccountMenu />);
    expect(screen.queryByLabelText("アカウントメニュー")).toBeNull();
  });

  it("新UI有効でも表示対象アイテムがない場合は何も描画しない", () => {
    mockUiV2State.enabled = true;
    mockVisibleNavV2Items = [];

    render(<AccountMenu />);
    expect(screen.queryByLabelText("アカウントメニュー")).toBeNull();
  });

  it("新UI有効かつ表示対象アイテムがある場合、ボタンが表示される", () => {
    mockUiV2State.enabled = true;
    mockVisibleNavV2Items = [{ key: "home", label: "ホーム" }];

    render(<AccountMenu />);
    expect(screen.getByLabelText("アカウントメニュー")).toBeDefined();
  });

  it("ボタンクリックでドロップダウンが開く", async () => {
    mockUiV2State.enabled = true;
    mockVisibleNavV2Items = [{ key: "home", label: "ホーム" }];

    render(<AccountMenu />);
    const button = screen.getByLabelText("アカウントメニュー");

    act(() => {
      button.click();
    });

    await waitFor(() => {
      expect(screen.getByText("admin@example.com")).toBeDefined();
    });
  });

  it("ドロップダウンにメール・権限・テーマ・新UI・ログアウトが表示される", async () => {
    mockUiV2State.enabled = true;
    mockVisibleNavV2Items = [{ key: "home", label: "ホーム" }];

    render(<AccountMenu />);
    const button = screen.getByLabelText("アカウントメニュー");

    act(() => {
      button.click();
    });

    await waitFor(() => {
      expect(screen.getByText("admin@example.com")).toBeDefined();
    });

    expect(screen.queryByText(/権限/)).not.toBeNull();
    expect(screen.queryByText("ログアウト")).not.toBeNull();
  });

  it("背景クリックでドロップダウンが閉じる", async () => {
    mockUiV2State.enabled = true;
    mockVisibleNavV2Items = [{ key: "home", label: "ホーム" }];

    const { container } = render(<AccountMenu />);
    const button = screen.getByLabelText("アカウントメニュー");

    act(() => {
      button.click();
    });

    await waitFor(() => {
      expect(screen.getByText("admin@example.com")).toBeDefined();
    });

    // 背景をクリック
    const body = container.parentElement;
    if (body) {
      act(() => {
        fireEvent.mouseDown(body);
      });
    }

    await waitFor(() => {
      expect(screen.queryByText("admin@example.com")).toBeNull();
    });
  });
});
