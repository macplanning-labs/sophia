"use client";

import Link from "next/link";
import type { ReadinessCheck } from "@/lib/guidance/settlement-readiness";
import { Check, AlertTriangle, X, Minus } from "lucide-react";

interface Props {
  checks: ReadinessCheck[];
  compact?: boolean;
}

function ChipIcon({ status }: { status: ReadinessCheck["status"] }) {
  const cls = "w-3 h-3 shrink-0";
  switch (status) {
    case "ok":
    case "done":
      return <Check className={`${cls} text-emerald-400`} />;
    case "warning":
      return <AlertTriangle className={`${cls} text-amber-400`} />;
    case "na":
      return <Minus className={`${cls} text-muted-foreground`} />;
    default:
      return <X className={`${cls} text-rose-400`} />;
  }
}

function chipClass(status: ReadinessCheck["status"]): string {
  switch (status) {
    case "ok":
      return "bg-emerald-500/10 border-emerald-500/25 text-emerald-300";
    case "done":
      return "bg-sky-500/10 border-sky-500/25 text-sky-300";
    case "warning":
      return "bg-amber-500/10 border-amber-500/25 text-amber-300";
    case "na":
      return "bg-muted/30 border-border text-muted-foreground";
    default:
      return "bg-rose-500/10 border-rose-500/25 text-rose-300";
  }
}

/** エンジニア行に表示する発行準備チェック（クリックで不足作業へ） */
export function IssueReadinessChips({ checks, compact = false }: Props) {
  if (checks.length === 0) return null;

  return (
    <div className={`flex flex-wrap gap-1 ${compact ? "mt-1 ml-4" : "mt-1.5 ml-4"}`}>
      {checks.map((check) => {
        const inner = (
          <span
            className={`inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[0.6rem] leading-tight ${chipClass(check.status)} ${
              check.linkPath ? "hover:brightness-110 cursor-pointer" : ""
            }`}
            title={check.detail}
          >
            <ChipIcon status={check.status} />
            <span>{check.label}</span>
          </span>
        );

        if (check.linkPath && (check.status === "missing" || check.status === "warning")) {
          return (
            <Link key={check.id} href={check.linkPath} onClick={(e) => e.stopPropagation()}>
              {inner}
            </Link>
          );
        }

        return <span key={check.id}>{inner}</span>;
      })}
    </div>
  );
}
