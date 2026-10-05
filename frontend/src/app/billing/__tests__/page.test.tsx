import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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

vi.mock("@/lib/api", () => ({
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
});
