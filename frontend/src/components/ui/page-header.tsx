"use client";

/**
 * ページ本体のツールバー（タイトルは AppHeader に表示するためここでは出さない）。
 * 左に subtitle(または絞り込み)、右に actions(ボタン)を置く。
 *
 * 全画面で本文の始まる位置をそろえるため、何も置かない画面でも同じ高さの行を出す(DEMO-000197)。
 * 各画面の本文の最初の要素として置く。
 */
interface PageHeaderProps {
  subtitle?: string;
  /** 左側に置く絞り込み・検索など(subtitle の代わり) */
  start?: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}

export function PageHeader({ subtitle, start, actions, className }: PageHeaderProps) {
  return (
    <div className={`flex items-center justify-between gap-3 min-h-9 mb-6 ${className ?? ""}`} data-page-header>
      <div className="flex items-center gap-2 min-w-0 flex-wrap">
        {start ?? (subtitle ? <p className="text-xs text-muted-foreground">{subtitle}</p> : null)}
      </div>
      {actions && <div className="flex items-center gap-2 shrink-0 flex-wrap justify-end">{actions}</div>}
    </div>
  );
}
