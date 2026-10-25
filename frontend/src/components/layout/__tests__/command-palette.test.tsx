import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act, waitFor } from "@testing-library/react";
import { CommandPalette, getAllNavItems } from "@/components/layout/command-palette";
import { CommandPaletteProvider } from "@/lib/CommandPaletteContext";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("@/lib/api", () => ({
  searchDocuments: vi.fn(async () => [
    { type: "invoice", id: "12", number: "INV-202609-001", title: "テスト請求先 2026年09月分 請求書", counterparty: "テスト請求先", status: "承認待ち", month: "2026-09", url: "/invoices/12" },
  ]),
  fetchMembers: vi.fn(async () => [
    { id: 7, name: "鈴木 花子", name_kana: "", affiliation_type: "PARTNER", partner_id: "P1", employee_id: "", email: "", is_active: true },
  ]),
  fetchMastersData: vi.fn(async (t: string) => (t === "clients" ? [{ id: 4, name: "テスト請求先" }] : [{ partner_id: "P1", name: "株式会社パートナー" }])),
}));

const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
function Wrap({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={qc}><CommandPaletteProvider>{children}</CommandPaletteProvider></QueryClientProvider>;
}

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
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    expect(screen.queryByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeNull();
  });

  it("⌘K で開く", async () => {
    render(
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // ⌘K を送信
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });
  });

  it("Ctrl+K でも開く", async () => {
    render(
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // Ctrl+K を送信
    act(() => {
      fireEvent.keyDown(document, { key: "k", ctrlKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });
  });

  it("入力で検索結果をフィルタリング", async () => {
    render(
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });

    const input = screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...");
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
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });

    // Esc を押す
    act(() => {
      fireEvent.keyDown(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索..."), {
        key: "Escape",
      });
    });

    await waitFor(() => {
      expect(screen.queryByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeNull();
    });
  });

  it("背景クリックで閉じる", async () => {
    const { container } = render(
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });

    // 背景（モーダルの外側）をクリック - 背景のdivではなく、その外側をクリック
    // ここでは、body 全体に mousedown イベントを発火させることで、
    // モーダルの外側をクリックしたことをシミュレート
    act(() => {
      fireEvent.mouseDown(document.body);
    });

    await waitFor(() => {
      expect(screen.queryByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeNull();
    });
  });

  it("結果がない場合は「該当のものが見つかりません」と表示", async () => {
    render(
      <Wrap>
        <CommandPalette />
      </Wrap>
    );

    // パレットを開く
    act(() => {
      fireEvent.keyDown(document, { key: "k", metaKey: true });
    });

    await waitFor(() => {
      expect(screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...")).toBeDefined();
    });

    const input = screen.getByPlaceholderText("ページ・帳票・要員・取引先を検索...");
    act(() => {
      fireEvent.change(input, { target: { value: " xxxxx" } });
    });

    await waitFor(() => {
      expect(screen.getByText("該当のものが見つかりません")).toBeDefined();
    });
  });

  it("入力すると、帳票・要員・取引先の結果がグループ分けして出る", async () => {
    render(<Wrap><CommandPalette /></Wrap>);
    act(() => { fireEvent.keyDown(document, { key: "k", metaKey: true }); });
    const input = await screen.findByPlaceholderText("ページ・帳票・要員・取引先を検索...");
    act(() => { fireEvent.change(input, { target: { value: "テスト" } }); });
    await waitFor(() => expect(screen.getByText(/INV-202609-001/)).toBeDefined());
    expect(screen.getByText("帳票")).toBeDefined();
    expect(screen.getByText("取引先")).toBeDefined();
    expect(screen.getByText("クライアント")).toBeDefined();
    act(() => { fireEvent.change(input, { target: { value: "鈴木" } }); });
    await waitFor(() => expect(screen.getByText("要員")).toBeDefined());
    expect(screen.getByText("パートナー要員")).toBeDefined();
  });
});

describe("getAllNavItems(⌘K で探せる画面。DEMO-000188)", () => {
  it("タブでまとめた画面も、1つずつ探せる(サブにセクション名)", () => {
    const items = getAllNavItems(true);
    expect(items).toContainEqual({ label: "ダッシュボード", href: "/", sub: "ホーム" });
    expect(items).toContainEqual({ label: "月次確定", href: "/settlement", sub: "月の締め" });
    expect(items).toContainEqual({ label: "取り込み・承認", href: "/timesheet-matching", sub: "勤務表" });
    expect(items).toContainEqual({ label: "受信", href: "/received-emails", sub: "メール" });
    expect(items).toContainEqual({ label: "送信", href: "/sent-emails", sub: "メール" });
    const hrefs = items.map((i) => i.href);
    expect(new Set(hrefs).size).toBe(hrefs.length);
  });

  it("一般社員には、管理者専用の画面を出さない", () => {
    const hrefs = getAllNavItems(false).map((i) => i.href);
    for (const adminOnly of ["/home", "/received-emails", "/billing", "/status", "/parties", "/users"]) {
      expect(hrefs, adminOnly).not.toContain(adminOnly);
    }
    expect(hrefs).toContain("/settlement");
  });
});
