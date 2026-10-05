"use client";

import { useEffect, useRef, ReactNode, useState } from "react";
import { X } from "lucide-react";
import { ConfirmSheet } from "@/components/ui/confirm-sheet";

interface SidePanelProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  dirty?: boolean;
}

export function SidePanel({ open, title, onClose, children, footer, dirty = false }: SidePanelProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const [showConfirm, setShowConfirm] = useState(false);

  // Focus management: store previous focus when opening
  useEffect(() => {
    if (open) {
      previousFocusRef.current = document.activeElement as HTMLElement;
      // Focus first input or close button
      const timer = setTimeout(() => {
        const inputs = panelRef.current?.querySelectorAll("input, textarea, select, [tabindex]");
        if (inputs && inputs.length > 0) {
          (inputs[0] as HTMLInputElement).focus?.();
        } else {
          (panelRef.current?.querySelector("button[aria-label='閉じる']") as HTMLButtonElement)?.focus?.();
        }
      }, 0);
      return () => clearTimeout(timer);
    }
  }, [open]);

  // Restore focus when closing
  useEffect(() => {
    if (!open && previousFocusRef.current) {
      previousFocusRef.current.focus();
    }
  }, [open]);

  // Handle Escape key
  useEffect(() => {
    if (!open) return;

    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        if (dirty) {
          setShowConfirm(true);
        } else {
          onClose();
        }
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [open, dirty, onClose]);

  if (!open) return null;

  return (
    <>
      <div
        className="fixed right-0 top-14 bottom-0 w-[480px] max-sm:w-full bg-card border-l border-border z-40 shadow-lg animate-in slide-in-from-right duration-200 flex flex-col"
        ref={panelRef}
        role="complementary"
        aria-label={title}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
          <h2 className="text-base font-semibold text-foreground">{title}</h2>
          <button
            onClick={() => (dirty ? setShowConfirm(true) : onClose())}
            aria-label="閉じる"
            className="text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4">{children}</div>

        {/* Footer */}
        {footer && <div className="px-6 py-4 border-t border-border shrink-0">{footer}</div>}
      </div>

      {/* Confirmation dialog for unsaved changes */}
      <ConfirmSheet
        open={showConfirm}
        title="入力中の内容は破棄されます"
        description="閉じますか?"
        confirmLabel="閉じる"
        cancelLabel="続ける"
        variant="danger"
        onConfirm={() => {
          setShowConfirm(false);
          onClose();
        }}
        onCancel={() => setShowConfirm(false)}
      />
    </>
  );
}
