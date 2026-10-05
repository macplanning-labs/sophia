import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { SidePanel } from "../side-panel";

afterEach(() => cleanup());

const base = { title: "テストパネル", onClose: vi.fn(), children: "Content" };

describe("SidePanel", () => {
  it("open=false で何も描画しない", () => {
    render(<SidePanel {...base} open={false} />);
    expect(screen.queryByText("テストパネル")).toBeNull();
  });

  it("open=true で見出しと子要素を表示する", () => {
    render(<SidePanel {...base} open />);
    expect(screen.getByText("テストパネル")).toBeDefined();
    expect(screen.getByText("Content")).toBeDefined();
  });

  it("role='complementary' で背景オーバーレイなしで実装", () => {
    const { container } = render(<SidePanel {...base} open />);
    const panel = container.querySelector("[role='complementary']");
    expect(panel).toBeDefined();
    // 背景を覆う fixed inset-0 要素が無いことを確認
    const backdropFixed = container.querySelector(".fixed.inset-0.bg-black");
    expect(backdropFixed).toBeNull();
  });

  it("Esc キーで onClose が呼ばれる（dirty でない場合）", async () => {
    const onClose = vi.fn();
    render(<SidePanel {...base} open dirty={false} onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("dirty=true で Esc キーを押すと確認ダイアログが出る（onClose は呼ばれない）", async () => {
    const onClose = vi.fn();
    render(<SidePanel {...base} open dirty onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText("入力中の内容は破棄されます")).toBeDefined();
  });

  it("確認ダイアログで「閉じる」を選ぶと onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    render(<SidePanel {...base} open dirty onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    const closeButtons = screen.getAllByRole("button", { name: "閉じる" });
    // ダイアログの「閉じる」ボタン（最後のもの）をクリック
    fireEvent.click(closeButtons[closeButtons.length - 1]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("閉じるボタンで onClose が呼ばれる（dirty でない場合）", async () => {
    const onClose = vi.fn();
    render(<SidePanel {...base} open dirty={false} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("footer が指定されたら表示される", () => {
    render(<SidePanel {...base} open footer={<button>Save</button>} />);
    expect(screen.getByRole("button", { name: "Save" })).toBeDefined();
  });

  it("開いたときに最初の入力要素にフォーカスが当たる", async () => {
    const { rerender } = render(
      <SidePanel {...base} open={false}>
        <input aria-label="test input" />
      </SidePanel>
    );
    rerender(
      <SidePanel {...base} open>
        <input aria-label="test input" />
      </SidePanel>
    );
    await new Promise((r) => setTimeout(r, 50));
    const input = screen.getByLabelText("test input");
    expect(document.activeElement).toBe(input);
  });

  it("title で aria-label が設定される", () => {
    const { container } = render(<SidePanel {...base} open />);
    const panel = container.querySelector("[role='complementary']");
    expect(panel?.getAttribute("aria-label")).toBe("テストパネル");
  });

  it("dirty=true で閉じるボタンを押すと確認ダイアログが出る", async () => {
    const onClose = vi.fn();
    render(<SidePanel {...base} open dirty onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "閉じる" }));
    expect(screen.getByText("入力中の内容は破棄されます")).toBeDefined();
  });
});
