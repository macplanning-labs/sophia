"use client";

interface SummaryCardProps {
  label: string;
  value: React.ReactNode;
  unit?: string;
  sub?: string;
  /** Tailwindテキスト色クラス */
  color?: string;
}

export function SummaryCard({
  label,
  value,
  unit,
  sub,
  color = "text-foreground",
}: SummaryCardProps) {
  return (
    <div className="bg-card border border-border rounded-[0.625rem] px-4 py-3.5 shadow-sm transition-all duration-200 hover:-translate-y-0.5 hover:border-border/80">
      <div className="text-[0.6875rem] text-muted-foreground font-medium uppercase tracking-wide mb-1">
        {label}
      </div>
      <div className={`text-xl font-bold tabular-nums leading-tight ${color}`}>
        {value}
        {unit && (
          <span className="text-xs text-muted-foreground font-normal ml-1">
            {unit}
          </span>
        )}
      </div>
      {sub && (
        <div className="text-[0.6875rem] text-muted-foreground mt-1">{sub}</div>
      )}
    </div>
  );
}

interface SummaryCardGridProps {
  children: React.ReactNode;
  /** カラム数。デフォルト auto-fit */
  cols?: number;
}

export function SummaryCardGrid({ children, cols }: SummaryCardGridProps) {
  const style = cols
    ? { gridTemplateColumns: `repeat(${cols}, 1fr)` }
    : { gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" };

  return (
    <div className="grid gap-3 mb-5" style={style}>
      {children}
    </div>
  );
}
