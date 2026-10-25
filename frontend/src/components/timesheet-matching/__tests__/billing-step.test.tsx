import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const item = (id: number, name: string) => ({
  client_contract_id: id, engineer_id: id, engineer_name: name, project_id: "P1", project_name: "基盤更改", timesheet_id: id, amount: 700000,
});
const missingItem = { client_contract_id: 3, engineer_name: "日高 直樹", project_id: "P1", project_name: "基盤更改", timesheet_status: null };
const base = {
  key: "c7", client_id: 7, client_name: "テスト商事", billing_unit: "CLIENT", project_id: null, subject: "10月分",
  items: [item(1, "山田 太郎"), item(2, "鈴木 花子")], subtotal: 1400000, tax_amount: 140000, total: 1540000,
  already_issued_count: 0, edi_excluded: false,
};
let invoice: Record<string, unknown>;
const confirmBilling = vi.fn();

vi.mock("@/lib/api", () => ({
  fetchBillingPreview: async () => ({ month: "2026-10", invoices: [invoice] }),
  confirmBilling: (req: unknown) => confirmBilling(req),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import { BillingStep } from "../billing-step";

const hint = { client_id: 7, key: "c7", month: "2026-10", missing_count: 0, confirmable: true };
function renderStep(onClose = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><BillingStep hint={hint} onClose={onClose} /></QueryClientProvider>);
  return onClose;
}

beforeEach(() => confirmBilling.mockReset());
afterEach(cleanup);

describe("BillingStep(承認後の請求書確認)", () => {
  it("全員揃っていれば「請求書を確定」で通常確定する(force なし)", async () => {
    invoice = { ...base, missing: [], confirmable: true };
    confirmBilling.mockResolvedValue({ success: true, created: [{ key: "c7", invoice_id: 1, invoice_no: "INV-1", total: 1540000 }], skipped: [] });
    renderStep();
    fireEvent.click(await screen.findByRole("button", { name: "請求書を確定" }));
    await waitFor(() => expect(confirmBilling).toHaveBeenCalledWith({ month: "2026-10", keys: ["c7"], force: false }));
    expect(await screen.findByText(/INV-1/)).toBeTruthy();
  });

  it("揃っていなければ「請求書を確定」は押せず、例外確定は理由と要員ごとの扱いが揃うまで押せない", async () => {
    invoice = { ...base, missing: [missingItem], confirmable: false };
    confirmBilling.mockResolvedValue({ success: true, created: [], skipped: [] });
    renderStep();
    const normal = await screen.findByRole("button", { name: "請求書を確定" });
    expect((normal as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("あと1名の勤務表が未承認")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "待たずに確定(例外)…" }));
    const go = screen.getByRole("button", { name: "待たずに確定" }) as HTMLButtonElement;
    expect(go.disabled).toBe(true);
    fireEvent.change(screen.getByPlaceholderText(/先方の締め日/), { target: { value: "締め日が早い" } });
    expect(go.disabled).toBe(true); // 要員の扱いが未選択
    fireEvent.change(screen.getByLabelText("日高 直樹の扱い"), { target: { value: "NEXT_MONTH" } });
    expect(go.disabled).toBe(false);
    fireEvent.click(go);
    await waitFor(() =>
      expect(confirmBilling).toHaveBeenCalledWith({
        month: "2026-10", keys: ["c7"], force: true, force_reason: "締め日が早い",
        force_actions: [{ client_contract_id: 3, action: "NEXT_MONTH" }],
      }),
    );
  });

  it("「あとで確認する(閉じる)」で閉じられる", async () => {
    invoice = { ...base, missing: [], confirmable: true };
    const onClose = renderStep();
    fireEvent.click(await screen.findByRole("button", { name: "あとで確認する(閉じる)" }));
    expect(onClose).toHaveBeenCalled();
  });
});
