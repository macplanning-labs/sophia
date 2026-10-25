import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MonthProvider } from "@/lib/MonthContext";

const cell = (tone: string, label: string) => ({ tone, label });
const base = {
  client_id: 1, project_id: "P", received_order: cell("ok", "受注登録"), purchase_order: cell("ok", "送付済"),
  timesheet: cell("ok", "承認済"), invoice: cell("ok", "発行済"), notice: cell("na", "自社社員"),
};
const matrix = {
  month: "2026-10",
  summary: { total: 3, needs_action: 2, invoice_unconfirmed: 1, timesheet_stalled: 1 },
  rows: [
    { ...base, engineer_id: 1, engineer_name: "山田 太郎", client_name: "テスト請求先", project_name: "朝日生命", invoice: cell("warn", "未確定"), next_action: "REVIEW_INVOICE" },
    { ...base, engineer_id: 2, engineer_name: "ピールマイディン", client_name: "クロス", project_name: "イオン", timesheet: cell("bad", "差し戻し"), next_action: "REREQUEST_TIMESHEET" },
    { ...base, engineer_id: 3, engineer_name: "佐藤 次郎", client_name: "テスト請求先", project_name: "朝日生命", next_action: null },
  ],
};
vi.mock("@/lib/api", () => ({ fetchStatusMatrix: vi.fn(async () => matrix) }));

import StatusPage from "../page";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><MonthProvider><StatusPage /></MonthProvider></QueryClientProvider>);
}
afterEach(cleanup);

describe("月別状況", () => {
  it("初期は「要対応のみ」。次の操作が、移動先つきで出る", async () => {
    renderPage();
    const link = await screen.findByRole("link", { name: "請求書を確認" });
    expect(link.getAttribute("href")).toBe("/billing");
    expect(screen.getByRole("link", { name: "再提出を依頼" }).getAttribute("href")).toBe("/timesheet-matching");
    expect(screen.queryByText("佐藤 次郎")).toBeNull(); // 対応なしは隠れる
  });

  it("「要対応のみ」を外すと、対応なしの行も出る。検索で絞れる", async () => {
    renderPage();
    await screen.findByText("山田 太郎");
    fireEvent.click(screen.getByLabelText("要対応のみ"));
    expect(screen.getByText("佐藤 次郎")).toBeDefined();
    fireEvent.change(screen.getByPlaceholderText("要員・取引先・案件で検索"), { target: { value: "ピール" } });
    await waitFor(() => expect(screen.queryByText("山田 太郎")).toBeNull());
    expect(screen.getByText("ピールマイディン")).toBeDefined();
  });
});
