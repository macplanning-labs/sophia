import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { MonthProvider } from "@/lib/MonthContext";
import { MonthSwitcher } from "@/components/layout/month-switcher";

afterEach(() => {
  cleanup();
  try { sessionStorage.clear(); } catch { /* 無視 */ }
});

function label(y: number, m: number) {
  return `${y}年${m}月`;
}

describe("MonthSwitcher", () => {
  it("今月を「YYYY年M月」の1か所だけで表示する(月が2つ並ばない)", () => {
    const now = new Date();
    render(<MonthProvider><MonthSwitcher /></MonthProvider>);
    expect(screen.getAllByText(label(now.getFullYear(), now.getMonth() + 1))).toHaveLength(1);
  });

  it("前月・翌月ボタンで月が変わる", async () => {
    const base = new Date(2026, 9, 15); // 2026年10月
    const y = base.getFullYear();
    window.history.replaceState({}, "", "/?month=2026-10");
    render(<MonthProvider><MonthSwitcher /></MonthProvider>);
    // ?month= の反映は、マウント後(ハイドレーション不一致を避けるため)
    expect(await screen.findByText(label(y, 10))).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: "翌月" }));
    expect(screen.getByText(label(y, 11))).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: "前月" }));
    fireEvent.click(screen.getByRole("button", { name: "前月" }));
    expect(screen.getByText(label(y, 9))).toBeDefined();
    window.history.replaceState({}, "", "/");
  });

  it("直接選択の入力があり、変更で月が変わる", () => {
    window.history.replaceState({}, "", "/");
    render(<MonthProvider><MonthSwitcher /></MonthProvider>);
    const input = screen.getByLabelText("月を選択") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "2027-03" } });
    expect(screen.getByText(label(2027, 3))).toBeDefined();
  });
});
