"use client";

import { useCallback, useEffect } from "react";
import { X, type LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

interface DetailModalProps {
  open: boolean;
  title: string;
  /** 絵文字文字列 or Lucide アイコン（旧 DetailLayout 本文タイトルと同型） */
  icon?: string | LucideIcon;
  subtitle?: string;
  /** モーダル幅 */
  size?: "md" | "lg" | "xl" | "2xl";
  onClose: () => void;
  children: React.ReactNode;
  className?: string;
}

const sizeMap = {
  md: "max-w-xl",
  lg: "max-w-3xl",
  xl: "max-w-5xl",
  "2xl": "max-w-6xl",
};

/** 閲覧・編集埋め込み用のフッターなしモーダル（?edit= パターン向け） */
export function DetailModal({
  open,
  title,
  icon,
  subtitle,
  size = "xl",
  onClose,
  children,
  className,
}: DetailModalProps) {
  const handleBackdrop = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose]
  );

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const IconComponent = icon && typeof icon !== "string" ? icon : null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center pt-[4vh] bg-black/60 backdrop-blur-sm overflow-y-auto"
      onClick={handleBackdrop}
    >
      <div
        className={cn(
          "bg-card border border-border rounded-xl shadow-2xl w-full mx-4 mb-10 animate-in fade-in zoom-in-95 duration-200 flex flex-col max-h-[92vh]",
          sizeMap[size],
          className
        )}
      >
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-3 border-b border-border shrink-0">
          <div className="min-w-0">
            <h2 className="text-xl font-bold text-foreground flex items-center gap-2 truncate">
              {IconComponent ? (
                <IconComponent className="w-5 h-5 text-primary shrink-0" />
              ) : icon ? (
                <span className="shrink-0">{icon as string}</span>
              ) : null}
              <span className="truncate">{title}</span>
            </h2>
            {subtitle && (
              <p className="text-xs text-muted-foreground mt-1 truncate">{subtitle}</p>
            )}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground transition-colors p-1 shrink-0"
            aria-label="閉じる"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
        <div className="px-4 py-4 overflow-y-auto flex-1 min-h-0">{children}</div>
      </div>
    </div>
  );
}
