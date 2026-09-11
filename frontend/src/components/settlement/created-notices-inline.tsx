"use client";

import { useEffect, useState } from "react";
import { X } from "lucide-react";
import type { CreatedNotice } from "@/lib/types";
import NoticeDetailPage from "@/app/notices/[id]/client";

interface Props {
  notices: CreatedNotice[];
  onDismiss: () => void;
}

function formatYen(n: number): string {
  return `¥${n.toLocaleString("ja-JP")}`;
}

/** 支払通知一括発行直後、パートナー行の下に表示するインラインパネル。
 *  PDFプレビュー・承認・送信までこの場で続けられるようにする。 */
export function CreatedNoticesInline({ notices, onDismiss }: Props) {
  const [activeId, setActiveId] = useState<string | null>(notices[0]?.notice_id ?? null);

  useEffect(() => {
    setActiveId(notices[0]?.notice_id ?? null);
  }, [notices]);

  if (notices.length === 0) return null;

  return (
    <div className="mx-3 my-3 rounded-lg border border-amber-500/40 bg-amber-500/[0.06] overflow-hidden">
      <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-amber-500/25 bg-amber-500/[0.08]">
        <div className="min-w-0">
          <p className="text-sm font-semibold text-foreground">
            支払通知を{notices.length}件作成しました
          </p>
          <p className="text-xs text-muted-foreground mt-0.5">
            まだパートナーには送信されていません。内容を確認のうえ、承認・送信してください。
          </p>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          className="text-muted-foreground hover:text-foreground transition-colors p-1 shrink-0"
          aria-label="パネルを閉じる"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {notices.length > 1 && (
        <div className="flex items-center gap-1 px-3 pt-2 overflow-x-auto border-b border-amber-500/20">
          {notices.map((n) => (
            <button
              key={n.notice_id}
              type="button"
              onClick={() => setActiveId(n.notice_id)}
              className={`px-3 py-1.5 text-xs font-medium rounded-t-md border-b-2 whitespace-nowrap transition-colors ${
                activeId === n.notice_id
                  ? "border-amber-400 text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              {n.project_name ? `${n.project_name} · ` : ""}
              {formatYen(n.total)}
              {n.needs_approval ? " · 要承認" : ""}
            </button>
          ))}
        </div>
      )}

      <div className="px-3 py-3 max-h-[min(60vh,520px)] overflow-y-auto">
        {activeId != null && (
          <NoticeDetailPage key={activeId} noticeId={activeId} embedded />
        )}
      </div>
    </div>
  );
}
