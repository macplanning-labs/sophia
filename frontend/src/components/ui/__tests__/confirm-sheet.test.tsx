import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { ConfirmSheet } from "../confirm-sheet";

afterEach(() => cleanup());

const base = { title: "請求書を削除", onConfirm: vi.fn(), onCancel: vi.fn() };

describe("ConfirmSheet", () => {
  it("閉じているときは何も表示しない", () => {
    render(<ConfirmSheet {...base} open={false} />);
    expect(screen.queryByText("請求書を削除")).toBeNull();
  });

  it("見出し・項目(ラベルと値)・補足・追加フォームを表示する", () => {
    render(
      <ConfirmSheet
        {...base}
        open
        facts={[
          { label: "宛先", value: "株式会社テスト" },
          { label: "金額", value: "1,520,000円" },
        ]}
        description="取り消せません"
      >
        <input aria-label="理由" />
      </ConfirmSheet>,
    );
    expect(screen.getByText("請求書を削除")).toBeDefined();
    expect(screen.getByText("宛先")).toBeDefined();
    expect(screen.getByText("株式会社テスト")).toBeDefined();
    expect(screen.getByText("1,520,000円")).toBeDefined();
    expect(screen.getByText("取り消せません")).toBeDefined();
    expect(screen.getByLabelText("理由")).toBeDefined();
  });

  it("確認ボタンで onConfirm、キャンセルで onCancel を呼ぶ", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<ConfirmSheet {...base} open confirmLabel="削除" onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("処理中は確認・キャンセルが無効で、「処理中...」と表示する", () => {
    render(<ConfirmSheet {...base} open loading confirmLabel="削除" />);
    expect(screen.getByRole("button", { name: "処理中..." })).toHaveProperty("disabled", true);
    expect(screen.getByRole("button", { name: "キャンセル" })).toHaveProperty("disabled", true);
  });

  it("danger は赤い確認ボタンになる", () => {
    render(<ConfirmSheet {...base} open variant="danger" confirmLabel="削除" />);
    expect(screen.getByRole("button", { name: "削除" }).className).toContain("bg-red-600");
  });
});
