"use client";

/**
 * ページ本体のツールバー（タイトルは AppHeader に表示するためここでは出さない）。
 * subtitle / actions のみ配置する。
 */
interface PageHeaderProps {
  subtitle?: string;
  actions?: React.ReactNode;
  className?: string;
}

export function PageHeader({ subtitle, actions, className }: PageHeaderProps) {
  if (!subtitle && !actions) return null;

  return (
    <div className={`flex items-start justify-between gap-3 mb-6 ${className ?? ""}`}>
      {subtitle ? (
        <p className="text-xs text-muted-foreground pt-1">{subtitle}</p>
      ) : (
        <span />
      )}
      {actions && <div className="flex items-center gap-2 shrink-0">{actions}</div>}
    </div>
  );
}
