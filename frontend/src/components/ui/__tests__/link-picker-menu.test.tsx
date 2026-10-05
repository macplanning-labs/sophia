import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { LinkPickerMenu, LinkPickerOption } from "../link-picker-menu";

afterEach(() => cleanup());

const baseOptions: LinkPickerOption[] = [
  { value: "opt-1", label: "Option 1", group: "Group A" },
  { value: "opt-2", label: "Option 2", group: "Group A" },
  { value: "opt-3", label: "Option 3", group: "Group B" },
];

describe("LinkPickerMenu", () => {
  it("ボタンを押すとメニューが開く", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    expect(screen.getByText("Option 1")).toBeDefined();
    expect(screen.getByText("Option 2")).toBeDefined();
    expect(screen.getByText("Option 3")).toBeDefined();
  });

  it("検索で絞り込める", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const searchInput = screen.getByPlaceholderText("検索...") as HTMLInputElement;
    fireEvent.change(searchInput, { target: { value: "Option 1" } });

    expect(screen.getByText("Option 1")).toBeDefined();
    expect(screen.queryByText("Option 2")).toBeNull();
    expect(screen.queryByText("Option 3")).toBeNull();
  });

  it("Enter で選択して onSelect が呼ばれる", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const searchInput = screen.getByPlaceholderText("検索...");
    fireEvent.keyDown(searchInput, { key: "Enter" });

    expect(onSelect).toHaveBeenCalledWith("opt-1");
  });

  it("Esc でメニューが閉じる", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    expect(screen.getByText("Option 1")).toBeDefined();

    const searchInput = screen.getByPlaceholderText("検索...");
    fireEvent.keyDown(searchInput, { key: "Escape" });

    expect(screen.queryByText("Option 1")).toBeNull();
  });

  it("外側クリックでメニューが閉じる", async () => {
    const onSelect = vi.fn();
    render(
      <div>
        <div data-testid="outside">Outside</div>
        <LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />
      </div>
    );

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    expect(screen.getByText("Option 1")).toBeDefined();

    const outside = screen.getByTestId("outside");
    fireEvent.click(outside);

    expect(screen.queryByText("Option 1")).toBeNull();
  });

  it("disabled 項目は選択できない", async () => {
    const onSelect = vi.fn();
    const options: LinkPickerOption[] = [
      { value: "opt-1", label: "Option 1" },
      { value: "opt-2", label: "Option 2", disabled: true },
    ];

    render(<LinkPickerMenu label="Select" options={options} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const opt2Button = screen.getByRole("button", { name: "Option 2" });
    expect(opt2Button).toHaveProperty("disabled", true);
  });

  it("group でオプションが分類される", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    expect(screen.getByText("Group A")).toBeDefined();
    expect(screen.getByText("Group B")).toBeDefined();
  });

  it("emptyText が表示される（マッチするオプションがない場合）", async () => {
    const onSelect = vi.fn();
    render(
      <LinkPickerMenu
        label="Select"
        options={baseOptions}
        onSelect={onSelect}
        emptyText="見つかりません"
      />
    );

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const searchInput = screen.getByPlaceholderText("検索...") as HTMLInputElement;
    fireEvent.change(searchInput, { target: { value: "zzz" } });

    expect(screen.getByText("見つかりません")).toBeDefined();
  });

  it("オプションをクリックすると onSelect が呼ばれてメニューが閉じる", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const opt2 = screen.getByRole("button", { name: "Option 2" });
    fireEvent.click(opt2);

    expect(onSelect).toHaveBeenCalledWith("opt-2");
    expect(screen.queryByText("Option 1")).toBeNull();
  });

  it("↑↓ キーでハイライト移動（オプション間）", async () => {
    const onSelect = vi.fn();
    render(<LinkPickerMenu label="Select" options={baseOptions} onSelect={onSelect} />);

    const button = screen.getByRole("button", { name: "Select" });
    fireEvent.click(button);

    const searchInput = screen.getByPlaceholderText("検索...");

    fireEvent.keyDown(searchInput, { key: "ArrowDown" });
    fireEvent.keyDown(searchInput, { key: "ArrowDown" });
    fireEvent.keyDown(searchInput, { key: "Enter" });

    // 最初は opt-1、↓ で opt-2、↓ で opt-3、Enter で opt-3 が選ばれる
    expect(onSelect).toHaveBeenCalledWith("opt-3");
  });
});
