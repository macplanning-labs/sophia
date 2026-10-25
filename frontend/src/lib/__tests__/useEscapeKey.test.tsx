import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent } from "@testing-library/react";
import { useEscapeKey } from "@/lib/useEscapeKey";
import { FormModal } from "@/components/ui/form-modal";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";

function Probe({ active, onEscape }: { active: boolean; onEscape: () => void }) {
  useEscapeKey(active, onEscape);
  return null;
}

afterEach(() => cleanup());

describe("useEscapeKey", () => {
  it("Esc で閉じる。active でなければ反応しない", () => {
    const a = vi.fn();
    const { rerender } = render(<Probe active onEscape={a} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(a).toHaveBeenCalledTimes(1);
    rerender(<Probe active={false} onEscape={a} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(a).toHaveBeenCalledTimes(1);
  });

  it("重なっているときは、いちばん手前だけが閉じる", () => {
    const back = vi.fn();
    const front = vi.fn();
    render(
      <>
        <Probe active onEscape={back} />
        <Probe active onEscape={front} />
      </>
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(front).toHaveBeenCalledTimes(1);
    expect(back).not.toHaveBeenCalled();
  });

  it("ほかのキーには反応しない", () => {
    const a = vi.fn();
    render(<Probe active onEscape={a} />);
    fireEvent.keyDown(document, { key: "Enter" });
    expect(a).not.toHaveBeenCalled();
  });
});

describe("小窓の Esc", () => {
  it("入力の小窓(FormModal)は Esc で閉じる。保存中は閉じない", () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <FormModal open title="t" onSubmit={() => {}} onClose={onClose}>x</FormModal>
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    rerender(<FormModal open loading title="t" onSubmit={() => {}} onClose={onClose}>x</FormModal>);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("確認の小窓(ConfirmDialog)は Esc でキャンセルする", () => {
    const onCancel = vi.fn();
    render(<ConfirmDialog open title="t" onConfirm={() => {}} onCancel={onCancel} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
