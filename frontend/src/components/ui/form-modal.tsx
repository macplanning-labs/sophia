"use client";

import { useCallback } from "react";
import { Button } from "@/components/ui/button";
import { X } from "lucide-react";

interface FormModalProps {
  open: boolean;
  title: string;
  /** モーダル幅: sm(max-w-md) | md(max-w-xl) | lg(max-w-3xl) | xl(max-w-5xl) */
  size?: "sm" | "md" | "lg" | "xl";
  loading?: boolean;
  submitLabel?: string;
  onSubmit: () => void;
  onClose: () => void;
  children: React.ReactNode;
  /** フッター左側(削除ボタン等) */
  footerStart?: React.ReactNode;
}

const sizeMap = {
  sm: "max-w-md",
  md: "max-w-xl",
  lg: "max-w-3xl",
  xl: "max-w-5xl",
};

export function FormModal({
  open,
  title,
  size = "md",
  loading = false,
  submitLabel = "保存",
  onSubmit,
  onClose,
  children,
  footerStart,
}: FormModalProps) {
  const handleBackdrop = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget && !loading) onClose();
    },
    [loading, onClose]
  );

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-start justify-center pt-[10vh] bg-black/60 backdrop-blur-sm overflow-y-auto"
      onClick={handleBackdrop}
    >
      <div
        className={`bg-card border border-border rounded-xl shadow-2xl w-full ${sizeMap[size]} mx-4 mb-10 animate-in fade-in zoom-in-95 duration-200`}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 pt-5 pb-3 border-b border-border">
          <h3 className="text-base font-semibold text-foreground">{title}</h3>
          <button
            onClick={onClose}
            disabled={loading}
            className="text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="px-6 py-4 space-y-4 max-h-[60vh] overflow-y-auto">
          {children}
        </div>

        {/* Footer */}
        <div className={`flex gap-2 px-6 py-4 border-t border-border ${footerStart ? "justify-between" : "justify-end"}`}>
          {footerStart && <div>{footerStart}</div>}
          <div className="flex gap-2 ml-auto">
            <Button
              variant="outline"
              size="sm"
              onClick={onClose}
              disabled={loading}
              className="border-border text-muted-foreground hover:text-foreground"
            >
              キャンセル
            </Button>
            <Button
              size="sm"
              onClick={onSubmit}
              disabled={loading}
              className="bg-primary hover:bg-primary/90 text-primary-foreground"
            >
              {loading ? "保存中..." : submitLabel}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── フォーム用入力コンポーネント ──

export interface FormFieldProps {
  label: string;
  required?: boolean;
  error?: string;
  className?: string;
  children: React.ReactNode;
}

export function FormField({ label, required, error, className, children }: FormFieldProps) {
  return (
    <div className={`space-y-1.5 ${className ?? ""}`}>
      <label className="text-xs font-medium text-muted-foreground">
        {label}
        {required && <span className="text-red-400 ml-0.5">*</span>}
      </label>
      {children}
      {error && <p className="text-xs text-red-400">{error}</p>}
    </div>
  );
}

interface FormInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  // extends standard input
}

export function FormInput({ className, ...props }: FormInputProps) {
  return (
    <input
      className={`w-full px-3 py-2 bg-muted border border-border rounded-md text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary disabled:opacity-50 ${className ?? ""}`}
      {...props}
    />
  );
}

interface FormSelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  options: { value: string; label: string }[];
  placeholder?: string;
}

export function FormSelect({ options, placeholder, className, ...props }: FormSelectProps) {
  return (
    <select
      className={`w-full px-3 py-2 bg-muted border border-border rounded-md text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary disabled:opacity-50 ${className ?? ""}`}
      {...props}
    >
      {placeholder && (
        <option value="" className="text-muted-foreground">
          {placeholder}
        </option>
      )}
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

interface FormTextareaProps extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  // extends standard textarea
}

export function FormTextarea({ className, ...props }: FormTextareaProps) {
  return (
    <textarea
      className={`w-full px-3 py-2 bg-muted border border-border rounded-md text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary disabled:opacity-50 min-h-[80px] ${className ?? ""}`}
      {...props}
    />
  );
}
