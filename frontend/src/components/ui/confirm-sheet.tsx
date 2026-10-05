"use client";

import type { ReactNode } from "react";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";

export type ConfirmFact = { label: string; value: ReactNode };

interface ConfirmSheetProps {
  open: boolean;
  title: string;
  /** 宛先・件数・金額・対象月など、「何を・誰に・いくつ・いくら」を示す項目 */
  facts?: ConfirmFact[];
  /** 補足(取り消せない、など) */
  description?: ReactNode;
  variant?: "danger" | "default";
  /** 名詞形(例: 削除、差し戻し、承認) */
  confirmLabel?: string;
  cancelLabel?: string;
  loading?: boolean;
  /** 理由入力などの追加フォーム */
  children?: ReactNode;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * window.confirm の置き換え用。何を対象に何が起きるかを、項目として見せてから確認させる。
 * 見た目と挙動(処理中は閉じない等)は ConfirmDialog を継承する。
 */
export function ConfirmSheet({
  facts,
  description,
  children,
  ...dialogProps
}: ConfirmSheetProps) {
  return (
    <ConfirmDialog
      {...dialogProps}
      description={
        <div className="space-y-3">
          {facts && facts.length > 0 && (
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-md border border-border bg-muted/30 px-3 py-2">
              {facts.map((f) => (
                <div key={f.label} className="contents">
                  <dt className="text-xs text-muted-foreground">{f.label}</dt>
                  <dd className="text-sm text-foreground break-words">{f.value}</dd>
                </div>
              ))}
            </dl>
          )}
          {description && <div>{description}</div>}
          {children}
        </div>
      }
    />
  );
}
