"use client";

import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertCircle, Mail } from "lucide-react";
import Link from "next/link";
import { fetchMailBriefs } from "@/lib/api";
import type { MailThreadBrief } from "@/lib/types";

interface Props {
  projectId?: string;
}

export function MailThreadCards({ projectId }: Props) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["mail-briefs"],
    queryFn: fetchMailBriefs,
    refetchInterval: 60000, // 60秒ごとに更新
  });

  if (isLoading) {
    return (
      <div className="space-y-3">
        {[1, 2].map((i) => (
          <div key={i} className="bg-card border border-border rounded-lg p-4 animate-pulse h-32" />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-4 flex gap-3">
        <AlertCircle className="h-5 w-5 text-destructive flex-shrink-0 mt-0.5" />
        <div>
          <p className="font-semibold text-sm text-destructive">取引先メールを読めませんでした</p>
          <p className="text-xs text-muted-foreground mt-1">{error instanceof Error ? error.message : "エラーが発生しました"}</p>
        </div>
      </div>
    );
  }

  if (!data || data.briefs.length === 0) {
    return null;
  }

  return (
    <div className="space-y-3">
      <h3 className="font-semibold text-sm text-foreground">取引先からのメール</h3>
      {data.briefs.map((brief) => (
        <MailThreadCard key={brief.from_email} brief={brief} />
      ))}
    </div>
  );
}

interface MailThreadCardProps {
  brief: MailThreadBrief;
}

function MailThreadCard({ brief }: MailThreadCardProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="bg-card border border-border rounded-lg p-4 hover:border-border-hover transition-colors">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left space-y-2"
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex-1 min-w-0">
            <div className="font-semibold text-sm text-foreground truncate">
              {brief.from_name}
              {brief.company_name && (
                <span className="text-xs text-muted-foreground ml-1">· {brief.company_name}</span>
              )}
            </div>
            <p className="text-xs text-muted-foreground mt-1 truncate">{brief.thread_subject}</p>
          </div>
          <Mail className="h-4 w-4 text-muted-foreground flex-shrink-0 mt-0.5" />
        </div>

        {/* サマリー表示 */}
        <div className="text-xs text-muted-foreground whitespace-pre-line line-clamp-2">
          {brief.summary}
        </div>

        {/* needs_choice インジケータ */}
        {brief.needs_choice && (
          <div className="text-xs bg-warning/10 text-warning border border-warning/30 rounded px-2 py-1 mt-2">
            見込みと最終があります。反映する方は担当者確認のうえ決めてください
          </div>
        )}

        {/* メール情報チップ */}
        <div className="flex gap-2 flex-wrap mt-2">
          {brief.attachments.slice(0, 3).map((att, idx) => (
            <div
              key={idx}
              className="text-xs bg-secondary/50 text-secondary-foreground rounded px-2 py-1 truncate"
            >
              {att.kind === "forecast" && "📋 見込み"}
              {att.kind === "final" && "✓ 最終"}
              {att.kind === "timesheet" && "📊 勤務表"}
              {att.kind === "other" && "📎 添付"}
              {att.hours_label && ` (${att.hours_label})`}
            </div>
          ))}
          <div className="text-xs text-muted-foreground py-1">
            {brief.email_count} 通
          </div>
        </div>
      </button>

      {/* 展開内容 */}
      {expanded && (
        <div className="mt-3 pt-3 border-t border-border space-y-2">
          {brief.attachments.slice(0, 5).map((att, idx) => (
            <div key={idx} className="text-xs text-muted-foreground">
              <span className="font-mono truncate block">{att.filename}</span>
              {att.hours_label && <span className="text-foreground">時間: {att.hours_label}</span>}
              {att.timesheet_status && <span className="text-foreground ml-2">状態: {att.timesheet_status}</span>}
            </div>
          ))}
          <Link
            href="/received-emails"
            className="inline-block text-xs text-primary hover:underline mt-2"
          >
            受信メールを見る →
          </Link>
        </div>
      )}
    </div>
  );
}
