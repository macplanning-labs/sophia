import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, cleanup, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const overview = {
  targets: { direct: 20, subcontract: 8 },
  projects: [
    {
      project_id: "PRJ1", project_name: "基盤更改", client_id: 1, client_name: "テスト商事",
      commercial_flow: "SUBCONTRACT", assignment_count: 2, internal_count: 1, revenue: 1_200_000, cost: 640_000,
      gross_profit: 560_000, margin_pct: 46.7, target_pct: 8, diff_pct: 38.7, status: "OK",
    },
    {
      project_id: "PRJ2", project_name: "新規開発", client_id: 1, client_name: "テスト商事",
      commercial_flow: null, assignment_count: 0, internal_count: 0, revenue: 0, cost: 0,
      gross_profit: 0, margin_pct: null, target_pct: null, diff_pct: null, status: "NOT_APPLICABLE",
    },
  ],
};
const members = [
  { id: 7, name: "山田 太郎", name_kana: "", affiliation_type: "PARTNER", partner_id: "0000000003", employee_id: "", email: "", is_active: true },
  { id: 8, name: "鈴木 花子", name_kana: "", affiliation_type: "EMPLOYEE", partner_id: null, employee_id: "", email: "", is_active: true },
];
const previewResult = {
  monthly: { revenue: 700000, cost: 720000, gross_profit: -20000, margin_pct: -2.9 },
  target: { flow: "SUBCONTRACT", target_pct: 8, diff_pct: -10.9, status: "BELOW" },
  hours_scenarios: [{ label: "標準", hours: 160, revenue: 700000, cost: 720000, margin_pct: -2.9 }],
  warnings: [{ code: "LOSS", message: "赤字です(月額 -20,000円)" }],
  project_total: {
    before: { revenue: 1200000, cost: 640000, gross_profit: 560000, margin_pct: 46.7 },
    after: { revenue: 1900000, cost: 1360000, gross_profit: 540000, margin_pct: 28.4 },
  },
};

const previewAssignment = vi.fn();
const createAssignment = vi.fn();

vi.mock("@/lib/api", () => ({
  fetchAssignmentOverview: async () => overview,
  fetchMembers: async () => members,
  fetchMastersData: async () => [{ partner_id: "0000000003", name: "株式会社パートナー" }],
  previewAssignment: (...a: unknown[]) => previewAssignment(...a),
  createAssignment: (...a: unknown[]) => createAssignment(...a),
}));
vi.mock("@/lib/useCurrentUser", () => ({ useCurrentUser: () => ({ isAdmin: true, isLoading: false }) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

import AssignmentsPage from "../page";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <AssignmentsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  previewAssignment.mockResolvedValue(previewResult);
  createAssignment.mockResolvedValue({ success: true, engineer_id: 7, client_contract_id: 1, partner_contract_id: 2 });
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

async function openPanelFor(memberName: string, projectName: string) {
  fireEvent.click(await screen.findByLabelText(`${memberName}を案件へ編成`));
  // 案件名は、カードとメニューの両方に出る。メニュー(後から描画される)の項目を選ぶ
  await screen.findAllByText(projectName);
  const matches = screen.getAllByText(projectName);
  fireEvent.click(matches[matches.length - 1]);
  await screen.findByRole("complementary");
}

describe("アサイン編成画面", () => {
  it("取引先ごとの案件カード(利益つき)と、要員の一覧が出る", async () => {
    renderPage();
    expect(await screen.findByText("基盤更改")).toBeDefined();
    expect(screen.getByText(/粗利 ¥560,000\(46\.7%\)/)).toBeDefined();
    expect(screen.getByText(/うち自社社員 1名。原価は含まず/)).toBeDefined();
    expect(screen.getByText("商流未設定")).toBeDefined();
    expect(await screen.findByText("山田 太郎")).toBeDefined();
    expect(screen.getByText("鈴木 花子")).toBeDefined();
  });

  it("ドラッグ無しで(「…」メニューから案件を選んで)サイドパネルが開く。背景を覆わない", async () => {
    const { container } = renderPage();
    await openPanelFor("山田 太郎", "基盤更改");
    const panel = screen.getByRole("complementary");
    expect(panel.getAttribute("aria-label")).toContain("山田 太郎");
    expect(container.querySelector(".fixed.inset-0")).toBeNull();
    // パートナー要員は発注条件(提案元パートナー)を持つ
    expect(screen.getByText("発注条件(提案元パートナー)")).toBeDefined();
    const create = screen.getByRole("button", { name: "アサイン作成" }) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
  });

  it("自社社員のパネルには発注条件が無い", async () => {
    renderPage();
    await openPanelFor("鈴木 花子", "新規開発");
    expect(screen.queryByText("発注条件(提案元パートナー)")).toBeNull();
  });

  it("単価を入力すると利益を試算して、警告と案件全体の変化を表示し、赤字でも作成できる", async () => {
    renderPage();
    await openPanelFor("鈴木 花子", "基盤更改");
    const price = screen.getAllByPlaceholderText("700000")[0];
    fireEvent.change(price, { target: { value: "700000" } });

    await waitFor(() => expect(previewAssignment).toHaveBeenCalled(), { timeout: 2000 });
    expect(await screen.findByText("赤字です(月額 -20,000円)")).toBeDefined();
    expect(screen.getByText(/目安8% を10\.9pt 下回る/)).toBeDefined();
    expect(screen.getByText(/この要員を加えると: 粗利 ¥540,000/)).toBeDefined();
    expect(screen.getByText("赤字ですが、作成できます。")).toBeDefined();

    const create = screen.getByRole("button", { name: "アサイン作成" }) as HTMLButtonElement;
    expect(create.disabled).toBe(false);
    fireEvent.click(create);
    await waitFor(() => expect(createAssignment).toHaveBeenCalledTimes(1));
    const body = createAssignment.mock.calls[0][0];
    expect(body).toMatchObject({ project_id: "PRJ1", staff_type: "EMPLOYEE", engineer_id: 8 });
    expect(body.partner_contract).toBeUndefined();
  });

  it("使い方の手順が画面に出ている(つかむ→案件カードへドラッグ→パネルで条件を入れる)", async () => {
    renderPage();
    const steps = await screen.findByRole("list", { name: "使い方" });
    expect(steps.textContent).toContain("右の要員をつかんで");
    expect(steps.textContent).toContain("左の案件カードへドラッグ");
    expect(steps.textContent).toContain("アサイン作成");
    expect(steps.textContent).toContain("要員の「案件を選ぶ」");
  });

  it("要員をつかむと、案件カードに「ここにドロップ」が出る", async () => {
    renderPage();
    const row = (await screen.findByLabelText("山田 太郎(ドラッグで案件へ)")) as HTMLElement;
    expect(screen.queryByText("ここにドロップ")).toBeNull();
    fireEvent.dragStart(row, { dataTransfer: { setData: () => {}, effectAllowed: "" } });
    expect((await screen.findAllByText("ここにドロップ")).length).toBe(2);
    fireEvent.dragEnd(row);
    await waitFor(() => expect(screen.queryByText("ここにドロップ")).toBeNull());
  });

  it("案件カードの「要員を選ぶ」から要員を選んで、その案件のパネルが開く", async () => {
    renderPage();
    fireEvent.click(await screen.findByLabelText("基盤更改に要員をアサイン"));
    await screen.findAllByText("鈴木 花子");
    // 開いたメニューは左の案件エリアの中にあり、右の要員エリアより先に描画される
    fireEvent.click(screen.getAllByText("鈴木 花子")[0]);
    const panel = await screen.findByRole("complementary");
    expect(panel.getAttribute("aria-label")).toContain("鈴木 花子");
    expect(screen.getByText("テスト商事 / 基盤更改")).toBeDefined();
  });
});

