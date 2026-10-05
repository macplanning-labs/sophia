import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, cleanup, act } from "@testing-library/react";
import { MonthProvider, useCurrentMonth } from "@/lib/MonthContext";

/**
 * useCurrentMonth フックを使用するテスト用コンポーネント
 */
function TestComponent() {
  const { month, setMonth } = useCurrentMonth();
  return (
    <div>
      <div data-testid="month">{month}</div>
      <button onClick={() => setMonth("2026-09")}>Set to September</button>
    </div>
  );
}

describe("MonthContext", () => {
  beforeEach(() => {
    // localStorage のクリア
    sessionStorage.clear();
    // URL のリセット
    delete (window as any).__test_url;
  });

  afterEach(() => {
    cleanup();
  });

  it("初期値は当月である", () => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, "0");
    const expected = `${year}-${month}`;

    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    expect(screen.getByTestId("month").textContent).toBe(expected);
  });

  it("sessionStorage に保存した値を読み込む", () => {
    sessionStorage.setItem("sophia-month", "2026-08");

    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    // マウント後に sessionStorage から読み込まれるまで待つ
    expect(screen.getByTestId("month").textContent).toBe("2026-08");
  });

  it("setMonth で月を変更できる", async () => {
    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    const button = screen.getByText("Set to September");
    act(() => {
      button.click();
    });

    expect(screen.getByTestId("month").textContent).toBe("2026-09");
  });

  it("setMonth で変更した値を sessionStorage に保存する", () => {
    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    const button = screen.getByText("Set to September");
    act(() => {
      button.click();
    });

    const stored = sessionStorage.getItem("sophia-month");
    expect(stored).toBe("2026-09");
  });

  it("不正な月形式を拒否する", () => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, "0");
    const expected = `${year}-${month}`;

    sessionStorage.setItem("sophia-month", "invalid");

    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    // 不正な値は無視され、当月が使用される
    expect(screen.getByTestId("month").textContent).toBe(expected);
  });

  it("sessionStorage へのアクセス失敗時は当月を使用する", () => {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, "0");
    const expected = `${year}-${month}`;

    // sessionStorage のアクセスを失敗させる
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("Storage access failed");
    });

    render(
      <MonthProvider>
        <TestComponent />
      </MonthProvider>
    );

    expect(screen.getByTestId("month").textContent).toBe(expected);

    vi.restoreAllMocks();
  });
});
