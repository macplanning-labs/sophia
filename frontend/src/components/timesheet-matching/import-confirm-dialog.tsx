"use client";

import { useEffect, useState } from "react";
import { useQuery, useMutation } from "@tanstack/react-query";
import { X } from "lucide-react";
import { toast } from "sonner";
import { TimesheetPreview, type TimesheetPreviewData } from "@/components/TimesheetPreview";

interface AlertRow {
  date: string;
  detail: string;
}

interface Preview {
  email: { from_name: string; from_email: string; subject: string };
  filename: string;
  worker_name: string;
  project_name: string;
  target_month: string;
  total_hours: string | number;
  work_days: number;
  daily_data: TimesheetPreviewData["daily_data"];
  holiday_dates?: string[];
  alerts: AlertRow[];
  order: { order_id: number; client_name: string; engineer_name: string; project_name: string };
  name_matches: boolean;
  project_matches: boolean | null;
  existing_status: string | null;
  can_import: boolean;
}

const STATUS_LABEL: Record<string, string> = {
  UPLOADED: "提出済",
  PARSED: "解析済",
  PENDING: "未提出（差戻し）",
  APPROVED: "承認済",
};

async function fetchPreview(attachmentId: number, orderId: number): Promise<Preview> {
  const r = await fetch(`/api/v1/timesheet-attachments/${attachmentId}/preview?order_id=${orderId}`);
  const b = await r.json().catch(() => ({}));
  if (!r.ok || !b.success) throw new Error(b.error || "勤務表を読み取れませんでした");
  return b.preview as Preview;
}

/**
 * 結び付けた勤務表の取り込み確認。既存の稼働報告アップロードと同じ確認画面（TimesheetPreview:
 * 左に原本、右に読み取り結果）に、結び付け先の受注を添えて表示する。「確認して承認する」で取り込み+承認。
 */
export function ImportConfirmDialog({
  attachmentId,
  orderId,
  onClose,
  onDone,
  onRejected,
}: {
  attachmentId: number;
  orderId: number;
  onClose: () => void;
  onDone: () => void;
  /** 差し戻したあと（一覧を更新して閉じる） */
  onRejected: () => void;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const [pdfUrl, setPdfUrl] = useState<string | null>(null);
  const [excelBuffer, setExcelBuffer] = useState<ArrayBuffer | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["timesheet-attachment-preview", attachmentId, orderId],
    queryFn: () => fetchPreview(attachmentId, orderId),
    retry: false,
  });

  // 原本（PDF / Excel）を取得して、左ペインに表示する
  useEffect(() => {
    if (!data) return;
    let cancelled = false;
    let url: string | null = null;
    (async () => {
      try {
        const r = await fetch(`/api/v1/timesheet-attachments/${attachmentId}/file`);
        if (!r.ok) return;
        if (/\.pdf$/i.test(data.filename)) {
          const blob = await r.blob();
          if (cancelled) return;
          url = URL.createObjectURL(new Blob([blob], { type: "application/pdf" }));
          setPdfUrl(url);
        } else {
          const buf = await r.arrayBuffer();
          if (!cancelled) setExcelBuffer(buf);
        }
      } catch {
        /* 原本が出せなくても、読み取り結果の確認と取り込みはできる */
      }
    })();
    return () => {
      cancelled = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [data, attachmentId]);

  const importMutation = useMutation({
    mutationFn: async () => {
      const r = await fetch(`/api/v1/timesheet-attachments/${attachmentId}/import`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ order_id: orderId }),
      });
      const b = await r.json().catch(() => ({}));
      if (!r.ok || !b.success) throw new Error(b.error || "取り込みに失敗しました");
    },
    onSuccess: () => {
      toast.success("稼働報告に取り込み、承認しました");
      onDone();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  // 差し戻しの理由の初期値: 合わない日など、画面の警告をそのまま使う
  const defaultReason = data
    ? [
        "勤務表の内容を確認しましたが、次の点が合いませんでした。",
        ...data.alerts.map((a) => `・${a.date ? `${a.date}: ` : ""}${a.detail}`),
        "お手数ですが、ご確認のうえ、修正した勤務表の再送をお願いします。",
      ].join("\n")
    : "";

  // 送信元へ渡す依頼文（コピーして、メールなどで人が送る。アプリからは送らない）
  const requestText = data
    ? `${data.email.from_name || "ご担当者"}様\n\nお世話になっております。\n「${data.email.subject}」でお送りいただいた勤務表（${data.filename}）について、\n${reason.trim() || defaultReason}`
    : "";

  const rejectMutation = useMutation({
    mutationFn: async () => {
      const r = await fetch(`/api/v1/timesheet-attachments/${attachmentId}/reject`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ reason: reason.trim() || defaultReason }),
      });
      const b = await r.json().catch(() => ({}));
      if (!r.ok || !b.success) throw new Error(b.error || "差し戻しに失敗しました");
    },
    onSuccess: () => {
      toast.success("差し戻しました。送信元へ再提出を依頼してください（依頼文はコピーできます）");
      onRejected();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const previewData: TimesheetPreviewData | null = data
    ? {
        worker_name: data.worker_name,
        target_month: `${data.target_month}-01`,
        total_hours: String(data.total_hours),
        work_days: data.work_days,
        overtime_hours: "0",
        sheet_name: "",
        original_filename: data.filename,
        has_times: true,
        alerts: data.alerts.map((a) => (a.date ? `${a.date}: ${a.detail}` : a.detail)),
        daily_data: data.daily_data,
        holiday_dates: data.holiday_dates ?? [],
      }
    : null;

  const linkSummary = data ? (
    <div className="border border-border rounded-lg px-3 py-1.5 bg-card text-xs flex flex-wrap items-center gap-x-4 gap-y-0.5">
      <span className="font-semibold text-foreground">結び付け先の受注</span>
      <span className="text-sm text-foreground">
        {data.order.client_name} / {data.order.engineer_name} / {data.order.project_name || "案件名なし"}
      </span>
      {!data.name_matches && (
        <span className="text-amber-400">氏名が違います（勤務表: {data.worker_name} / 受注: {data.order.engineer_name}）</span>
      )}
      {data.project_matches === false && (
        <span className="text-muted-foreground">勤務表の案件名「{data.project_name}」は受注と違います（結び付けの条件ではありません）</span>
      )}
      {data.existing_status && (
        <span className="text-amber-400">
          同じ月の稼働報告あり（{STATUS_LABEL[data.existing_status] ?? data.existing_status}）
          {data.can_import ? "→ 取り込むと置き換えます" : "→ 承認済みのため取り込めません"}
        </span>
      )}
      <span className="text-muted-foreground ml-auto">受領した勤務表が正です。時間はここで修正できません。</span>
    </div>
  ) : null;

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      {/* 画面の高さに収める（スクロールなしで、1枚のスクリーンショットに収まるように） */}
      <div className="bg-background border border-border rounded-xl shadow-2xl w-full max-w-[1600px] mx-3 h-[calc(100vh-1.5rem)] flex flex-col">
        <div className="flex items-center justify-between px-4 py-2 border-b border-border shrink-0">
          <h3 className="text-sm font-semibold text-foreground">稼働報告取り込み</h3>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground transition-colors" aria-label="閉じる">
            <X className="w-5 h-5" />
          </button>
        </div>
        <div className="px-4 py-2 flex-1 min-h-0 flex flex-col gap-2">
          {isLoading && <p className="text-sm text-muted-foreground">勤務表を読み取っています...</p>}
          {error && (
            <p className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-md p-3 whitespace-pre-wrap">
              {(error as Error).message}
            </p>
          )}
          {previewData && (
            <div className={rejecting ? "hidden" : "flex-1 min-h-0"}>
            <TimesheetPreview
              fit
              preview={previewData}
              pdfUrl={pdfUrl}
              excelBuffer={excelBuffer}
              header={linkSummary}
              extraActions={
                <button
                  onClick={() => { setReason(defaultReason); setRejecting(true); }}
                  className="px-4 py-2 text-sm font-medium text-red-400 border border-red-500/40 rounded-lg hover:bg-red-500/10 transition-colors"
                  title="勤務表の記載に誤りがあるとき、承認せずに、送信元へ再提出を依頼する"
                >
                  ↩ 差し戻し
                </button>
              }
              confirmLabel="確認して承認する"
              canConfirm={!!data?.can_import}
              isConfirming={importMutation.isPending}
              onConfirm={() => importMutation.mutate()}
              onCancel={onClose}
            />
            </div>
          )}
          {rejecting && data && (
            <div className="flex-1 min-h-0 overflow-auto">
          <div className="w-full max-w-2xl mx-auto mt-6 p-4 space-y-3 border border-border rounded-xl bg-card">
            <h4 className="text-sm font-semibold text-foreground">勤務表を差し戻す</h4>
            <p className="text-xs text-muted-foreground">
              承認せずに、送信元（{data.email.from_name || data.email.from_email}）へ再提出を依頼します。この勤務表は「差し戻し済み」になり、取り込み待ちから外れます。
              メールは自動では送られません。下の依頼文をコピーして、送信元へ送ってください。
            </p>
            <label className="block text-xs text-muted-foreground">
              差し戻しの理由（依頼文に入ります）
              <textarea
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                rows={7}
                className="mt-1 w-full px-2 py-1.5 text-sm bg-muted border border-border rounded-md text-foreground"
              />
            </label>
            <div className="flex items-center justify-between gap-2">
              <button
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(requestText);
                    toast.success("依頼文をコピーしました");
                  } catch {
                    toast.error("コピーできませんでした");
                  }
                }}
                className="px-3 py-1.5 text-xs font-medium rounded-md border border-border text-foreground hover:bg-muted transition-colors"
              >
                依頼文をコピー
              </button>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setRejecting(false)}
                  className="px-3 py-1.5 text-xs font-medium rounded-md border border-border text-foreground hover:bg-muted transition-colors"
                >
                  戻る
                </button>
                <button
                  onClick={() => rejectMutation.mutate()}
                  disabled={rejectMutation.isPending}
                  className="px-3 py-1.5 text-xs font-bold rounded-md text-white bg-red-600 hover:bg-red-500 transition-colors disabled:opacity-50"
                >
                  差し戻す
                </button>
              </div>
            </div>
          </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
