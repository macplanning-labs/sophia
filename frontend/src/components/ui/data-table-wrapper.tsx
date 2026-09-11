"use client";

interface DataTableWrapperProps {
  isLoading?: boolean;
  isEmpty?: boolean;
  emptyMessage?: string;
  /** フッターに表示する件数。undefined の場合フッター非表示 */
  count?: number;
  children: React.ReactNode;
}

/** スケルトンローディング */
function TableSkeleton() {
  return (
    <div className="p-6 space-y-3">
      {[...Array(5)].map((_, i) => (
        <div
          key={i}
          className="h-10 bg-muted/50 rounded animate-pulse"
          style={{ animationDelay: `${i * 80}ms` }}
        />
      ))}
    </div>
  );
}

/**
 * テーブルラッパー — ローディング・空状態・件数フッターを統一
 *
 * ```tsx
 * <DataTableWrapper isLoading={isLoading} isEmpty={!data.length} count={data.length}>
 *   <Table>...</Table>
 * </DataTableWrapper>
 * ```
 */
export function DataTableWrapper({
  isLoading,
  isEmpty,
  emptyMessage = "データがありません",
  count,
  children,
}: DataTableWrapperProps) {
  return (
    <div className="bg-card border border-border rounded-[0.625rem] overflow-hidden shadow-sm">
      {isLoading ? (
        <TableSkeleton />
      ) : isEmpty ? (
        <div className="py-16 text-center text-muted-foreground text-sm">
          {emptyMessage}
        </div>
      ) : (
        children
      )}
      {count !== undefined && !isLoading && (
        <div className="px-4 py-2 border-t border-border bg-background/60 text-xs text-muted-foreground">
          表示中: {count}件
        </div>
      )}
    </div>
  );
}
