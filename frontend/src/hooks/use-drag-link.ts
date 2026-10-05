import { useState, useCallback, useRef } from "react";

interface UseDragLinkOptions {
  onLink: (itemId: string, targetId: string) => void;
  onPick?: (itemId: string) => void;
}

export interface DragProps {
  draggable: boolean;
  onDragStart: (e: React.DragEvent<HTMLElement>) => void;
  onDragEnd: (e: React.DragEvent<HTMLElement>) => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLElement>) => void;
  tabIndex: number;
}

export interface DropProps {
  onDragOver: (e: React.DragEvent<HTMLElement>) => void;
  onDragEnter: (e: React.DragEvent<HTMLElement>) => void;
  onDragLeave: (e: React.DragEvent<HTMLElement>) => void;
  onDrop: (e: React.DragEvent<HTMLElement>) => void;
}

interface UseDragLinkReturn {
  dragProps: (itemId: string) => DragProps;
  dropProps: (targetId: string, opts?: { canDrop?: (itemId: string) => boolean }) => DropProps;
  draggingId: string | null;
  overTargetId: string | null;
}

export function useDragLink({
  onLink,
  onPick,
}: UseDragLinkOptions): UseDragLinkReturn {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [overTargetId, setOverTargetId] = useState<string | null>(null);
  const draggedItemRef = useRef<string | null>(null);

  const dragProps = useCallback(
    (itemId: string): DragProps => ({
      draggable: true,
      onDragStart: (e) => {
        e.dataTransfer.effectAllowed = "link";
        e.dataTransfer.setData("text/plain", itemId);
        draggedItemRef.current = itemId;
        setDraggingId(itemId);
      },
      onDragEnd: () => {
        draggedItemRef.current = null;
        setDraggingId(null);
      },
      onKeyDown: (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onPick?.(itemId);
        }
      },
      tabIndex: 0,
    }),
    [onPick]
  );

  const dropProps = useCallback(
    (targetId: string, opts?: { canDrop?: (itemId: string) => boolean }): DropProps => {
      const canDropItem = (itemId: string) => !opts?.canDrop || opts.canDrop(itemId);

      return {
        onDragOver: (e) => {
          const itemId = e.dataTransfer.getData("text/plain") || draggedItemRef.current;
          if (itemId && canDropItem(itemId)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "link";
          }
        },
        onDragEnter: (e) => {
          const itemId = e.dataTransfer.getData("text/plain") || draggedItemRef.current;
          if (itemId && canDropItem(itemId)) {
            e.preventDefault();
            setOverTargetId(targetId);
          }
        },
        onDragLeave: (e) => {
          if (e.currentTarget === e.target) {
            setOverTargetId(null);
          }
        },
        onDrop: (e) => {
          e.preventDefault();
          const itemId = e.dataTransfer.getData("text/plain") || draggedItemRef.current;
          if (itemId && canDropItem(itemId)) {
            onLink(itemId, targetId);
          }
          setOverTargetId(null);
        },
      };
    },
    [onLink]
  );

  return {
    dragProps,
    dropProps,
    draggingId,
    overTargetId,
  };
}
