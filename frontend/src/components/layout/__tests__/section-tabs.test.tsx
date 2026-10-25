import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, within } from "@testing-library/react";
import { SectionTabs } from "@/components/layout/section-tabs";

let mockPathname = "/users";
vi.mock("next/navigation", () => ({
  usePathname: () => mockPathname,
}));

let mockIsAdmin = true;
vi.mock("@/lib/useCurrentUser", () => ({
  useCurrentUser: () => ({
    isAdmin: mockIsAdmin,
    user: { email: "u@example.com", role: mockIsAdmin ? "ADMIN" : "EMPLOYEE" },
    isLoading: false,
    isError: false,
  }),
}));

afterEach(() => {
  cleanup();
  mockPathname = "/users";
  mockIsAdmin = true;
});

const tabTexts = () => screen.getAllByRole("tab").map((t) => t.textContent);
const selectedText = () => {
  const selected = screen.getAllByRole("tab").filter((t) => t.getAttribute("aria-selected") === "true");
  expect(selected).toHaveLength(1);
  expect(selected[0].getAttribute("aria-current")).toBe("page");
  return selected[0].textContent;
};

describe("SectionTabs", () => {
  it("設定(管理者): 7つのタブが並び、いまの画面のタブが選択中になる", () => {
    mockPathname = "/settings/org/roles";
    render(<SectionTabs />);
    expect(tabTexts()).toEqual(["ユーザー管理", "組織・ロール", "セキュリティ", "APIキー", "外部連携", "マスタメンテ", "取引先"]);
    expect(selectedText()).toBe("組織・ロール");
  });

  it("勤務表(管理者): 3つのタブ。各タブは従来のURLへのリンク", () => {
    mockPathname = "/timesheets";
    render(<SectionTabs />);
    const hrefs = Object.fromEntries(screen.getAllByRole("tab").map((t) => [t.textContent, t.getAttribute("href")]));
    expect(hrefs).toEqual({
      "取り込み・承認": "/timesheet-matching",
      "稼働報告": "/timesheets",
      "自己申告": "/my-timesheet",
    });
    expect(selectedText()).toBe("稼働報告");
  });

  it("月の締め(管理者): 5つのタブ", () => {
    mockPathname = "/billing";
    render(<SectionTabs />);
    expect(tabTexts()).toEqual(["月別状況", "月次確定", "請求書の確定", "請求書一覧", "Peppolログ"]);
    expect(selectedText()).toBe("請求書の確定");
  });

  it("ホーム: ダッシュボード(/)でもタブが出る", () => {
    mockPathname = "/";
    render(<SectionTabs />);
    expect(tabTexts()).toEqual(["要対応", "ダッシュボード"]);
    expect(selectedText()).toBe("ダッシュボード");
  });

  it("一般社員: 管理者専用のタブは出ない", () => {
    mockIsAdmin = false;
    mockPathname = "/settlement";
    render(<SectionTabs />);
    expect(tabTexts()).toEqual(["月次確定", "請求書一覧"]);
    expect(selectedText()).toBe("月次確定");
  });

  it("一般社員(設定): セキュリティのタブだけが出る", () => {
    mockIsAdmin = false;
    mockPathname = "/settings/security";
    render(<SectionTabs />);
    expect(tabTexts()).toEqual(["セキュリティ"]);
  });

  it("セクションに属さない画面では、何も出ない", () => {
    for (const p of ["/projects", "/orders/3", "/notices"]) {
      mockPathname = p;
      const { container, unmount } = render(<SectionTabs />);
      expect(container.innerHTML, p).toBe("");
      unmount();
    }
  });

  it("ログイン・ポータルなど、ヘッダーを出さない画面では、何も出ない", () => {
    for (const p of ["/login", "/portal/timesheet-entry", "/token/abc"]) {
      mockPathname = p;
      const { container, unmount } = render(<SectionTabs />);
      expect(container.innerHTML, p).toBe("");
      unmount();
    }
  });

  it("ナビゲーションとして、セクションの名前が付いている", () => {
    mockPathname = "/payroll";
    render(<SectionTabs />);
    const nav = screen.getByRole("navigation", { name: "社員・給与" });
    expect(within(nav).getAllByRole("tab").length).toBe(3);
  });
});
