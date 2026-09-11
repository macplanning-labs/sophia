"use client";

import { useRouter } from "next/navigation";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { LucideIcon } from "lucide-react";

interface Props {
  title: string;
  icon: string | LucideIcon;
  backHref: string;
  backLabel: string;
  isLoading: boolean;
  actions?: React.ReactNode;
  children: React.ReactNode;
  /** true: 一覧の行展開などへ埋め込み。戻るボタンと余白を抑える */
  embedded?: boolean;
}

export function DetailLayout({ title, icon, backHref, backLabel, isLoading, actions, children, embedded = false }: Props) {
  const router = useRouter();
  const IconComponent = typeof icon === "string" ? null : icon;

  if (isLoading) {
    return (
      <div className={embedded ? "space-y-4 py-2" : "p-6 space-y-6"}>
        <div className="h-8 w-64 bg-muted/50 rounded animate-pulse" />
        <div className="space-y-4">
          {[...Array(6)].map((_, i) => (
            <div key={i} className="h-12 bg-muted/50 rounded animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className={embedded ? "space-y-4 py-1" : "p-6 space-y-6"}>
      {/* embedded 時はタイトルを DetailModal ヘッダーへ移すため出さない。actions のみ残す */}
      {(embedded ? !!actions : true) && (
        <div className="flex items-center gap-4 flex-wrap">
          {!embedded && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => router.push(backHref)}
              className="text-muted-foreground hover:text-foreground"
            >
              <ArrowLeft className="w-4 h-4 mr-1" />
              {backLabel}
            </Button>
          )}
          {!embedded && (
            <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
              {IconComponent ? (
                <IconComponent className="w-5 h-5 text-primary" />
              ) : (
                <span>{icon as string}</span>
              )}
              {title}
            </h1>
          )}
          {actions}
        </div>
      )}
      {children}
    </div>
  );
}

/** 詳細フィールド表示用 */
export function Field({ label, value, className }: { label: string; value: React.ReactNode; className?: string }) {
  return (
    <div className={className}>
      <dt className="text-[11px] text-muted-foreground font-medium mb-0.5">{label}</dt>
      <dd className="text-sm text-foreground">{value ?? "—"}</dd>
    </div>
  );
}

/** フィールドグリッド */
export function FieldGrid({ children, cols = 4 }: { children: React.ReactNode; cols?: number }) {
  const gridClass = cols === 3 ? "grid-cols-3" : cols === 2 ? "grid-cols-2" : "grid-cols-4";
  return (
    <dl className={`grid ${gridClass} gap-4 bg-card border border-border rounded-lg p-4`}>
      {children}
    </dl>
  );
}
