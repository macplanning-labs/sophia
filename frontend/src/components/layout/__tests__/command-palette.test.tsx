import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act, waitFor } from "@testing-library/react";
import { CommandPalette } from "@/components/layout/command-palette";
import { CommandPaletteProvider } from "@/lib/CommandPaletteContext";

// useRouter のモック
vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: vi.fn(),
  }),
}));

// useCurrentUser のモック
vi.mock("@/lib/useCurrentUser", () => ({
  useCurrentUser: () => ({
    isAdmin: true,
    user: { email: "admin@example.com", role: "ADMIN" },
    isLoading: false,
    isError: false,
  }),
}));

describe("CommandPalette", () => {
  afterEach(() => {
    cleanup();
  });

  it("初期状態では表示されない", () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    expect(screen.queryByPlaceholderText("ページを検索...")).toBeNull();
  });

  it("⌘K で開く", async () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // ⌘K を送信
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });
  });

  it("Ctrl+K でも開く", async () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // Ctrl+K を送信
    act(() => {
      fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });
  });

  it("入力で検索結果をフィルタリング", async () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });

    const input = screen.getByPlaceholderText("ページを検索...");
    act(() => {
      fireEvent.change(input, { target: { value: "ダッシュボード" } });
    });

    // ダッシュボードが表示される
    await waitFor(() => {
      expect(screen.getByText("ダッシュボード")).toBeDefined();
    });
  });

  it("Esc で閉じる", async () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });

    // Esc を押す
    act(() => {
      fireEvent.keyDown(screen.getByPlaceholderText("ページを検索..."), {
        key: "Escape",
      });
    });

    await waitFor(() => {
      expect(screen.queryByPlaceholderText("ページを検索...")).toBeNull();
    });
  });

  it("背景クリックで閉じる", async () => {
    const { container } = render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });

    // 背景（モーダルの外側）をクリック - 背景のdivではなく、その外側をクリック
    // ここでは、body 全体に mousedown イベントを発火させることで、
    // モーダルの外側をクリックしたことをシミュレート
    act(() => {
      fireEvent.mouseDown(document.body);
    });

    await waitFor(() => {
      expect(screen.queryByPlaceholderText("ページを検索...")).toBeNull();
    });
  });

  it("結果がない場合は「該当のページが見つかりません」と表示", async () => {
    render(
      <CommandPaletteProvider>
        <CommandPalette />
      </CommandPaletteProvider>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページを検索...")).toBeDefined();
    });

    const input = screen.getByPlaceholderText("ページを検索...");
    act(() => {
      fireEvent.change(input, { target: { value: " xxxxx" } });
    });

    await waitFor(() => {
      expect(screen.getByText("該当のページが見つかりません")).toBeDefined();
    });
  });
});
