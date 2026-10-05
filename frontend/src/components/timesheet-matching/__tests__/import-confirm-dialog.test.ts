import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// 取り込み確認ダイアログは、差し戻し理由を「ダイアログの中」で開く(上に重ねない)。UI刷新 1-7 / DEMO-000130
describe("ImportConfirmDialog の構造", () => {
  const src = readFileSync(join(__dirname, "..", "import-confirm-dialog.tsx"), "utf-8");

  it("全画面のオーバーレイ(fixed inset-0)は1つだけ(差し戻し理由が二重モーダルにならない)", () => {
    const overlays = src.match(/fixed inset-0/g) ?? [];
    expect(overlays).toHaveLength(1);
  });

  it("差し戻し理由の欄と、戻る・差し戻すボタンが残っている", () => {
    expect(src).toContain("差し戻しの理由（依頼文に入ります）");
    expect(src).toContain("依頼文をコピー");
    expect(src).toContain("rejectMutation.mutate()");
  });
});
