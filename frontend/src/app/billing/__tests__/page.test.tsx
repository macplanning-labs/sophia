import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MonthProvider } from "@/lib/MonthContext";
import BillingPage from "../page";

// Mock modules
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("@/lib/useCurrentUser", () => ({
  useCurrentUser: () => ({ isAdmin: true, isLoading: false }),
}));

// 画面の月は「今月」が初期値なので、テストの請求書も今月・先月で作る(月が変わっても壊れない)
const ym = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
const THIS_MONTH = ym(new Date());
const LAST_MONTH = ym(new Date(new Date().getFullYear(), new Date().getMonth() - 1, 1));

vi.mock("next/navigation", () => ({
  useSearchParams: () => new URLSearchParams("key=c1%3ApPRJ001"),
}));

vi.mock("@/lib/api", () => ({
  fetchInvoices: vi.fn(() =>
    Promise.resolve([
      { id: 9, invoice_id: "INV-202610-009", client_name: "取引先1", subject: "s", project_name: null, issue_date: "2026-10-31",
        due_date: null, target_month: `${THIS_MONTH}-01`, total_amount: 990000, item_count: 1, status: "PENDING_APPROVAL" },
      { id: 8, invoice_id: "INV-202609-001", client_name: "取引先2", subject: "s", project_name: null, issue_date: "2026-09-30",
        due_date: null, target_month: `${LAST_MONTH}-01`, total_amount: 100, item_count: 1, status: "SENT" },
    ]),
  ),
  fetchBillingPreview: vi.fn(() =>
    Promise.resolve({
      month: "2026-10",
      invoices: [
        {
          key: "c1:pPRJ001",
          client_id: 1,
          client_name: "取引先1",
          billing_unit: "PROJECT",
          project_id: "PRJ001",
          subject: "案件1 2026年10月分 請求書",
          items: [
            {
              client_contract_id: 1,
              engineer_id: 1,
              engineer_name: "要員1",
              project_id: "PRJ001",
              project_name: "案件1",
              timesheet_id: 1,
              amount: 600000,
            },
          ],
          subtotal: 600000,
          tax_amount: 60000,
          total: 660000,
          missing: [],
          already_issued_count: 0,
          confirmable: true,
          edi_excluded: false,
        },
      ],
    })
  ),
  confirmBilling: vi.fn(() =>
    Promise.resolve({
      success: true,
      created: [
        {
          key: "c1:pPRJ001",
          invoice_id: 1,
          invoice_no: "INV-202610-001",
          total: 660000,
        },
      ],
      skipped: [],
    })
  ),
}));

describe("BillingPage", () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
  });

  it("should display notice about payment notices", () => {
    render(
      <QueryClientProvider client={queryClient}>
        <MonthProvider>
          <BillingPage />
        </MonthProvider>
      </QueryClientProvider>
    );

    // Check for info box about notices
    const noticeText = screen.getByText(/支払通知は/);
    expect(noticeText).toBeDefined();
  });

  it("確定済みの請求書を、その月のものだけ並べる(請求書の詳細へのリンクつき)", async () => {
    render(
      <QueryClientProvider client={queryClient}>
        <MonthProvider>
          <BillingPage />
        </MonthProvider>
      </QueryClientProvider>
    );
    await waitFor(() => expect(screen.queryByText("確定済みの請求書(この月)")).not.toBeNull());
    const link = screen.getAllByRole("link", { name: "請求書を開く →" });
    expect(link).toHaveLength(1);
    expect(link[0].getAttribute("href")).toBe("/invoices/9");
    expect(screen.queryByText("INV-202609-001")).toBeNull();
  });
});
