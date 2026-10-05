import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { SearchableColumnHeader } from "../searchable-column-header";

afterEach(() => {
  cleanup();
});

const mockOptions = [
  { value: "project-a", label: "プロジェクトA" },
  { value: "project-b", label: "プロジェクトB" },
  { value: "project-c", label: "プロジェクトC" },
];

describe("SearchableColumnHeader", () => {
  it("未選択時に列名が見える（「すべて」がトリガーに出ない）", () => {
    const handleChange = vi.fn();
    render(
      <SearchableColumnHeader
        label="プロジェクト名"
        options={mockOptions}
        value=""
        onChange={handleChange}
        allLabel="すべて"
        allowClear={true}
      />
    );

    // トリガーボタンを取得
    const buttons = screen.getAllByRole("button");
    const trigger = buttons[0]; // 最初のボタンがトリガー

    // 列名はトリガーに表示される
    expect(trigger.textContent).toContain("プロジェクト名");
    // 「すべて」がトリガーに表示されないことを確認
    expect(trigger.textContent).not.toContain("すべて");
  });

  it("値を選択中は列名と値が両方見える", () => {
    const handleChange = vi.fn();
    render(
      <SearchableColumnHeader
        label="プロジェクト名"
        options={mockOptions}
        value="project-a"
        onChange={handleChange}
        allLabel="すべて"
        allowClear={true}
      />
    );

    // トリガーボタンを取得
    const buttons = screen.getAllByRole("button");
    const trigger = buttons[0]; // 最初のボタンがトリガー

    // 列名が表示される
    expect(trigger.textContent).toContain("プロジェクト名");
    // 選択された値もバッジで表示される
    expect(trigger.textContent).toContain("プロジェクトA");
  });

  it("ポップオーバーを開くと先頭に「すべて」がある", async () => {
    const handleChange = vi.fn();
    render(
      <SearchableColumnHeader
        label="プロジェクト名"
        options={mockOptions}
        value=""
        onChange={handleChange}
        allLabel="すべて"
        allowClear={true}
      />
    );

    // トリガーボタンを取得
    const buttons = screen.getAllByRole("button");
    const trigger = buttons[0]; // 最初のボタンがトリガー

    // ポップオーバーを開く
    fireEvent.click(trigger);

    // 先頭に「すべて」が表示されることを確認
    const allLabelButton = screen.getByRole("button", { name: "すべて" });
    expect(allLabelButton).toBeDefined();
  });

  it("オプションに value=\"\" の「すべて」があっても、未選択ではバッジを出さない", () => {
    render(
      <SearchableColumnHeader
        label="状態"
        options={[{ value: "", label: "すべて" }, ...mockOptions]}
        value=""
        onChange={vi.fn()}
      />
    );
    const trigger = screen.getAllByRole("button")[0];
    expect(trigger.textContent).toContain("状態");
    expect(trigger.textContent).not.toContain("すべて");
  });
});
