import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MonthProvider } from "@/lib/MonthContext";
import { buildHomeCards } from "@/lib/home-cards";

const cell = (tone: string, label: string) => ({ tone, label });
const row = (over: Record<string, unknown>) => ({
  engineer_id: 1, engineer_name: "山田 太郎", client_id: 1, client_name: "テスト請求先", project_id: "P", project_name: "朝日",
  received_order: cell("ok", "受注登録"), purchase_order: cell("ok", "送付済"), timesheet: cell("ok", "承認済"),
  invoice: cell("ok", "発行済"), notice: cell("na", "自社社員"), next_action: null, ...over,
});
const matrix = (rows: unknown[]) => ({ month: "2026-10", summary: { total: rows.length, needs_action: 0, invoice_unconfirmed: 0, timesheet_stalled: 0 }, rows }) as never;
const inv = (status: string, month: string) => ({ id: 5, invoice_id: "INV-5", client_name: "テスト請求先", subject: "件名", project_name: null, issue_date: "", due_date: null, target_month: `${month}-01`, total_amount: 990000, item_count: 1, status });

describe("buildHomeCards", () => {
  it("件数のある種類だけを返す。請求書の確定待ちは、確定できる請求書の数", () => {
    const m = matrix([
      row({ timesheet: cell("bad", "差し戻し") }),
      row({ timesheet: cell("warn", "確認待ち"), invoice: cell("warn", "未確定") }),
      row({ received_order: cell("bad", "注文書なし") }),
    ]);
    const cards = buildHomeCards(m, [inv("PENDING_APPROVAL", "2026-10"), inv("PENDING_APPROVAL", "2026-09")], 2, "2026-10", 4);
    expect(cards.map((c) => [c.key, c.count])).toEqual([
      ["ts-returned", 1], ["ts-review", 1], ["inv-confirm", 4], ["inv-approve", 1], ["ro", 1], ["mail", 2],
    ]);
    expect(cards[0].tone).toBe("bad");
  });

  it("すべて0件なら空", () => {
    expect(buildHomeCards(matrix([row({})]), [inv("SENT", "2026-10")], 0, "2026-10", 0)).toEqual([]);
    expect(buildHomeCards(undefined, [], 0, "2026-10", 0)).toEqual([]);
  });
});

const api = vi.hoisted(() => ({
  rows: [] as unknown[],
  invoices: [] as unknown[],
  previews: [] as unknown[],
  mails: [] as unknown[],
  mailArgs: [] as boolean[],
  approveInvoice: vi.fn(async (_id: string) => ({ success: true })),
  rejectInvoice: vi.fn(async (_id: string) => ({ success: true })),
  confirmBilling: vi.fn(async (_req: unknown) => ({ success: true, created: [], skipped: [] })),
  apiPost: vi.fn(async (_u: string, _b: unknown) => ({ success: true })),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("@/lib/api", () => ({
  fetchStatusMatrix: async () => matrix(api.rows),
  fetchInvoices: async () => api.invoices,
  fetchReceivedEmails: (needsReview: boolean, knownDomainOnly: boolean) => { api.mailArgs = [needsReview, knownDomainOnly]; return Promise.resolve({ emails: api.mails }); },
  fetchBillingPreview: async () => ({ month: "x", invoices: api.previews }),
  confirmBilling: (r: unknown) => api.confirmBilling(r),
  approveInvoice: (id: string) => api.approveInvoice(id),
  rejectInvoice: (id: string) => api.rejectInvoice(id),
  apiPost: (u: string, b: unknown) => api.apiPost(u, b),
}));
import HomePage from "../page";

// 画面の月は「今月」が初期値。テストの請求書も今月で作る(月が変わっても壊れない)
const THIS_MONTH = `${new Date().getFullYear()}-${String(new Date().getMonth() + 1).padStart(2, "0")}`;
function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><MonthProvider><HomePage /></MonthProvider></QueryClientProvider>);
}
beforeEach(() => { api.rows = []; api.invoices = []; api.previews = []; api.mails = []; vi.clearAllMocks(); });
afterEach(cleanup);

describe("ホーム", () => {
  it("要対応がなければ「ありません」を出す", async () => {
    api.rows = [row({})];
    renderPage();
    expect(await screen.findByText(/手当てが必要なものは、ありません/)).toBeDefined();
  });

  it("カードを選ぶと、同じ画面の下の一覧が切り替わる(初期は先頭のカード)", async () => {
    api.rows = [row({ timesheet: cell("bad", "差し戻し") }), row({ engineer_name: "日高 直樹", received_order: cell("bad", "注文書なし") })];
    renderPage();
    const list = await screen.findByLabelText("一覧");
    expect(within(list).getByText("山田 太郎")).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /受注書/ }));
    await waitFor(() => expect(within(screen.getByLabelText("一覧")).getByText("日高 直樹")).toBeDefined());
    expect(within(screen.getByLabelText("一覧")).queryByText("山田 太郎")).toBeNull();
  });

  it("請求書の承認待ち: 「承認」→確認→実行で承認APIを呼ぶ(押しただけでは呼ばない)", async () => {
    api.invoices = [inv("PENDING_APPROVAL", THIS_MONTH)];
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "承認" }));
    expect(api.approveInvoice).not.toHaveBeenCalled();
    const buttons = await screen.findAllByRole("button", { name: "承認" });
    fireEvent.click(buttons[buttons.length - 1]); // 確認の「承認」
    await waitFor(() => expect(api.approveInvoice).toHaveBeenCalledWith("5"));
  });

  it("請求書の確定待ち: 確定できる請求書だけが出て、「請求書を確定」は通常確定(force なし)", async () => {
    api.previews = [
      { key: "c1", client_id: 1, client_name: "テスト請求先", subject: "10月分", items: [{}], total: 1100, missing: [], confirmable: true, edi_excluded: false },
      { key: "c2", client_id: 2, client_name: "別の取引先", subject: "待ち", items: [{}], total: 500, missing: [{}], confirmable: false, edi_excluded: false },
    ];
    renderPage();
    const list = await screen.findByLabelText("一覧");
    expect(within(list).getByText("テスト請求先")).toBeDefined();
    expect(within(list).queryByText("別の取引先")).toBeNull();
    fireEvent.click(within(list).getByRole("button", { name: "請求書を確定" }));
    const buttons = await screen.findAllByRole("button", { name: "確定" });
    fireEvent.click(buttons[buttons.length - 1]);
    await waitFor(() => expect(api.confirmBilling).toHaveBeenCalledWith(expect.objectContaining({ keys: ["c1"], force: false })));
  });

  it("受信メールのカードは、受信メールの画面の初期表示と同じ条件(要確認 + 取引先ドメインのみ)で数える", async () => {
    api.mails = [{ id: 1, from_name: "x", from_email: "x@example.com", subject: "s", received_at: "2026-09-01T00:00:00Z" }];
    renderPage();
    await screen.findByLabelText("一覧");
    expect(api.mailArgs).toEqual([true, true]);
  });
});
