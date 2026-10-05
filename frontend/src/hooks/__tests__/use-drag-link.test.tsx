import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent } from "@testing-library/react";
import { useDragLink, type DragProps, type DropProps } from "../use-drag-link";

afterEach(() => cleanup());

describe("useDragLink", () => {
  it("dragProps でドラッグ要素の props を返す", () => {
    let dragPropsResult = null as DragProps | null;

    function TestComponent() {
      const { dragProps } = useDragLink({ onLink: vi.fn() });
      dragPropsResult = dragProps("item-1");
      return null;
    }

    render(<TestComponent />);

    if (!dragPropsResult) throw new Error("dragPropsResult is null");
    expect(dragPropsResult.draggable).toBe(true);
    expect(typeof dragPropsResult.onDragStart).toBe("function");
    expect(typeof dragPropsResult.onDragEnd).toBe("function");
    expect(typeof dragPropsResult.onKeyDown).toBe("function");
    expect(dragPropsResult.tabIndex).toBe(0);
  });

  it("dropProps でドロップ要素の props を返す", () => {
    let dropPropsResult = null as DropProps | null;

    function TestComponent() {
      const { dropProps } = useDragLink({ onLink: vi.fn() });
      dropPropsResult = dropProps("target-1");
      return null;
    }

    render(<TestComponent />);

    if (!dropPropsResult) throw new Error("dropPropsResult is null");
    expect(typeof dropPropsResult.onDragOver).toBe("function");
    expect(typeof dropPropsResult.onDragEnter).toBe("function");
    expect(typeof dropPropsResult.onDragLeave).toBe("function");
    expect(typeof dropPropsResult.onDrop).toBe("function");
  });

  it("ドラッグを開始すると draggingId が設定される", () => {
    const onLink = vi.fn();
    const DragComponent = () => {
      const { dragProps, draggingId } = useDragLink({ onLink });
      const props = dragProps("item-1");

      return (
        <div>
          <div {...props} data-testid="drag-item">
            Item
          </div>
          <span data-testid="dragging-id">{draggingId}</span>
        </div>
      );
    };

    render(<DragComponent />);

    const dragItem = screen.getByTestId("drag-item");
    const mockDataTransfer = { setData: vi.fn(), effectAllowed: "link" };

    fireEvent.dragStart(dragItem, { dataTransfer: mockDataTransfer });

    expect(screen.getByTestId("dragging-id")).toBeDefined();
  });

  it("ドラッグ終了後は draggingId がクリアされる", () => {
    const onLink = vi.fn();
    const DragComponent = () => {
      const { dragProps } = useDragLink({ onLink });
      const props = dragProps("item-1");

      return (
        <div {...props} data-testid="drag-item">
          Item
        </div>
      );
    };

    const { rerender } = render(<DragComponent />);

    const dragItem = screen.getByTestId("drag-item");
    const mockDataTransfer = { setData: vi.fn(), effectAllowed: "link" };

    fireEvent.dragStart(dragItem, { dataTransfer: mockDataTransfer });
    fireEvent.dragEnd(dragItem);

    rerender(<DragComponent />);
  });

  it("ドロップで onLink が呼ばれる（canDrop チェック含む）", () => {
    const onLink = vi.fn();

    const DropComponent = () => {
      const { dropProps } = useDragLink({ onLink });
      const props = dropProps("target-1", { canDrop: (itemId) => itemId === "valid-item" });

      return (
        <div {...props} data-testid="drop-target">
          Target
        </div>
      );
    };

    render(<DropComponent />);

    const dropTarget = screen.getByTestId("drop-target");
    const mockDataTransfer = { getData: () => "valid-item", dropEffect: "link" };

    fireEvent.drop(dropTarget, { dataTransfer: mockDataTransfer });

    expect(onLink).toHaveBeenCalledWith("valid-item", "target-1");
  });

  it("canDrop=false でドロップできない", () => {
    const onLink = vi.fn();

    const DropComponent = () => {
      const { dropProps } = useDragLink({ onLink });
      const props = dropProps("target-1", { canDrop: () => false });

      return (
        <div {...props} data-testid="drop-target">
          Target
        </div>
      );
    };

    render(<DropComponent />);

    const dropTarget = screen.getByTestId("drop-target");
    const mockDataTransfer = { getData: () => "item-1", dropEffect: "link" };

    fireEvent.drop(dropTarget, { dataTransfer: mockDataTransfer });

    expect(onLink).not.toHaveBeenCalled();
  });

  it("onPick が呼ばれる（Enter キーで）", () => {
    const onPick = vi.fn();
    const onLink = vi.fn();

    const PickComponent = () => {
      const { dragProps } = useDragLink({ onLink, onPick });
      const props = dragProps("item-1");

      return (
        <div {...props} data-testid="pick-item">
          Item
        </div>
      );
    };

    render(<PickComponent />);

    const pickItem = screen.getByTestId("pick-item");
    fireEvent.keyDown(pickItem, { key: "Enter" });

    expect(onPick).toHaveBeenCalledWith("item-1");
  });

  it("onPick が呼ばれる（Space キーで）", () => {
    const onPick = vi.fn();
    const onLink = vi.fn();

    const PickComponent = () => {
      const { dragProps } = useDragLink({ onLink, onPick });
      const props = dragProps("item-1");

      return (
        <div {...props} data-testid="pick-item">
          Item
        </div>
      );
    };

    render(<PickComponent />);

    const pickItem = screen.getByTestId("pick-item");
    fireEvent.keyDown(pickItem, { key: " " });

    expect(onPick).toHaveBeenCalledWith("item-1");
  });
});
