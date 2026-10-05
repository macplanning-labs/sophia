"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
import { FileText, GripVertical, X } from "lucide-react";
import { PageHeader } from "@/components/ui/page-header";
import { Badge } from "@/components/ui/badge";
import { ImportConfirmDialog } from "@/components/timesheet-matching/import-confirm-dialog";

interface Order {
  order_id: number;
  client_id: number;
  client_name: string;
  engineer_name: string;
  project_name: string;
  order_status: string;
  client_contract_id: number | null;
  can_link: boolean;
  timesheet_id: number | null;
  timesheet_status: string | null;
  timesheet_hours: number | null;
}

interface Parsed {
  ok: boolean;
  error?: string;
  worker_name?: string;
  project_name?: string;
  target_month?: string;
  total_hours?: string | number;
  work_days?: number;
}

interface Attachment {
  attachment_id: number;
  email_id: number;
  filename: string;
  subject: string;
  from_name: string;
  from_email: string;
  received_at: string;
  sender_kind: "client" | "partner" | "unknown";
  sender_name: string;
  parsed: Parsed;
  imported_timesheet_id: number | null;
  suggested_order_id: number | null;
  candidate_order_ids: number[];
  superseded: boolean;
  /** 勤務表の氏名が社員マスタにあるか。無ければ表記の違い、あればその月の受注が未登録 */
  engineer_known: boolean;
  /** 差し戻し済み（承認せずに、送信元へ再提出を依頼した） */
  rejected: boolean;
  rejected_reason: string;
}

interface Board {
  month: string;
  orders: Order[];
  attachments: Attachment[];
}

const TS_STATUS_LABEL: Record<string, string> = {
  UPLOADED: "提出済",
  PARSED: "解析済",
  PENDING: "未提出（差戻し）",
  APPROVED: "承認済",
};

const SENDER_LABEL: Record<string, string> = { client: "取引先", partner: "パートナー", unknown: "送信元不明" };

/** 月末〜月初は、直前の月の勤務表を取り込むことが多いため、月初10日までは前月を初期表示にする */
function defaultMonth(): string {
  const d = new Date();
  if (d.getDate() <= 10) d.setMonth(d.getMonth() - 1);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

async function fetchBoard(month: string): Promise<Board> {
  const r = await fetch(`/api/v1/timesheet-matching?month=${encodeURIComponent(month)}`);
  const b = await r.json().catch(() => ({}));
  if (!r.ok || !b.success) throw new Error(b.error || "勤務表の一覧を取得できませんでした");
  return b.board as Board;
}

export default function TimesheetMatchingPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <TimesheetMatchingContent />
    </Suspense>
  );
}

function TimesheetMatchingContent() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const searchParams = useSearchParams();
  const [month, setMonth] = useState(searchParams.get("month") || defaultMonth());
  // 勤務表(添付ID) → 結び付けた受注ID。自動で決まったものは最初から入る
  const [links, setLinks] = useState<Record<number, number>>({});
  const [touched, setTouched] = useState<Set<number>>(new Set());
  const [dragId, setDragId] = useState<number | null>(null);
  const [confirm, setConfirm] = useState<{ attachmentId: number; orderId: number } | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["timesheet-matching", month],
    queryFn: () => fetchBoard(month),
  });

  // 月を変えたら、結び付けをやり直す
  useEffect(() => {
    setLinks({});
    setTouched(new Set());
  }, [month]);

  // 自動で決まった結び付けを取り込む（人が触ったものは上書きしない）
  useEffect(() => {
    if (!data) return;
    setLinks((prev) => {
      const next = { ...prev };
      const usedOrders = new Set(Object.values(next));
      for (const a of data.attachments) {
        if (a.imported_timesheet_id || touched.has(a.attachment_id) || next[a.attachment_id]) continue;
        const oid = a.suggested_order_id;
        if (oid && !usedOrders.has(oid)) {
          next[a.attachment_id] = oid;
          usedOrders.add(oid);
        }
      }
      return next;
    });
  }, [data, touched]);

  const ordersById = useMemo(() => new Map((data?.orders ?? []).map((o) => [o.order_id, o])), [data]);
  const groups = useMemo(() => {
    const m = new Map<string, Order[]>();
    for (const o of data?.orders ?? []) {
      const list = m.get(o.client_name) ?? [];
      list.push(o);
      m.set(o.client_name, list);
    }
    return [...m.entries()];
  }, [data]);

  const rejectedAtts = (data?.attachments ?? []).filter((a) => a.rejected && !a.imported_timesheet_id);
  const pendingAtts = (data?.attachments ?? []).filter((a) => !a.imported_timesheet_id && !a.rejected);
  const importedAtts = (data?.attachments ?? []).filter((a) => a.imported_timesheet_id);
  const unlinkedAtts = pendingAtts.filter((a) => !links[a.attachment_id]);
  const attByOrder = (orderId: number) => pendingAtts.find((a) => links[a.attachment_id] === orderId);

  const link = (attachmentId: number, orderId: number) => {
    const order = ordersById.get(orderId);
    if (!order || !order.can_link || order.timesheet_status === "APPROVED") return;
    setTouched((t) => new Set(t).add(attachmentId));
    setLinks((prev) => {
      const next: Record<number, number> = {};
      for (const [k, v] of Object.entries(prev)) {
        // 1受注に結び付けられる勤務表は1件。先に結び付いていたものは右に戻る
        if (v !== orderId) next[Number(k)] = v;
      }
      next[attachmentId] = orderId;
      return next;
    });
  };
  const unlink = (attachmentId: number) => {
    setTouched((t) => new Set(t).add(attachmentId));
    setLinks((prev) => {
      const next = { ...prev };
      delete next[attachmentId];
      return next;
    });
  };

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["timesheet-matching"] });

  const unreject = async (attachmentId: number) => {
    const r = await fetch(`/api/v1/timesheet-attachments/${attachmentId}/unreject`, { method: "POST" });
    if (r.ok) refresh();
  };

  return (
    <div className="space-y-4">
      <PageHeader
        subtitle="左の受注に、右の勤務表をドラッグして結び付けます。氏名（空白の違いは無視）と月が合うものは、最初から結び付いています"
        actions={
          <div className="flex items-center gap-2">
            <input
              type="month"
              value={month}
              onChange={(e) => e.target.value && setMonth(e.target.value)}
              className="px-2 py-1.5 text-sm bg-muted border border-border rounded-lg text-foreground"
              aria-label="対象月"
            />
            <Link
              href="/settlement"
              className="px-3 py-1.5 text-xs font-medium rounded-lg border border-border text-foreground hover:bg-muted transition-colors"
            >
              月次確定へ →
            </Link>
          </div>
        }
      />

      {isLoading && <p className="text-sm text-muted-foreground">読み込み中...</p>}
      {error && <p className="text-sm text-red-400">{(error as Error).message}</p>}

      {data && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 items-start">
          {/* 左: 取引先ごとの受注 */}
          <section className="bg-card border border-border rounded-lg p-3 space-y-3" aria-label="受注">
            <h2 className="text-sm font-semibold text-foreground">受注（取引先ごと・人・案件）　{data.month}</h2>
            {groups.length === 0 && <p className="text-xs text-muted-foreground">この月の受注がありません。</p>}
            {groups.map(([client, orders]) => (
              <div key={client} className="space-y-1.5">
                <p className="text-xs font-semibold text-muted-foreground">{client}</p>
                {orders.map((o) => {
                  const att = attByOrder(o.order_id);
                  const approved = o.timesheet_status === "APPROVED";
                  const droppable = o.can_link && !approved;
                  return (
                    <div
                      key={o.order_id}
                      onDragOver={(e) => { if (droppable && dragId !== null) e.preventDefault(); }}
                      onDrop={(e) => {
                        e.preventDefault();
                        const id = Number(e.dataTransfer.getData("text/plain")) || dragId;
                        if (id) link(id, o.order_id);
                        setDragId(null);
                      }}
                      className={`rounded-md border px-3 py-2 text-sm ${
                        att ? "border-emerald-500/40 bg-emerald-500/5" : "border-border bg-background"
                      } ${dragId !== null && droppable ? "border-dashed border-primary/60" : ""}`}
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="min-w-0">
                          <span className="font-medium text-foreground">{o.engineer_name || "（人未設定）"}</span>
                          <span className="ml-2 text-xs text-muted-foreground truncate">{o.project_name || "案件名なし"}</span>
                        </div>
                        <div className="flex items-center gap-1.5 shrink-0">
                          {o.timesheet_status && (
                            <Badge variant="outline" className="text-[10px]">
                              稼働報告: {TS_STATUS_LABEL[o.timesheet_status] ?? o.timesheet_status}
                              {o.timesheet_hours != null ? `（${o.timesheet_hours}h）` : ""}
                            </Badge>
                          )}
                          {!o.can_link && <Badge variant="outline" className="text-[10px] text-amber-400">契約未設定</Badge>}
                        </div>
                      </div>
                      {att ? (
                        <div
                          draggable
                          onDragStart={(e) => { e.dataTransfer.setData("text/plain", String(att.attachment_id)); setDragId(att.attachment_id); }}
                          onDragEnd={() => setDragId(null)}
                          className="mt-2 flex items-center justify-between gap-2 rounded-md bg-background border border-border px-2 py-1.5 cursor-grab"
                        >
                          <div className="min-w-0 text-xs">
                            <FileText className="inline w-3.5 h-3.5 mr-1 text-muted-foreground" />
                            <span className="text-foreground">{att.parsed.worker_name || att.filename}</span>
                            <span className="ml-2 text-muted-foreground">{Number(att.parsed.total_hours ?? 0)}h ・ {att.sender_name || att.from_name}</span>
                            {att.parsed.worker_name && att.candidate_order_ids.length === 0 && (
                              <span className="ml-2 text-amber-400">氏名が受注と一致しません</span>
                            )}
                          </div>
                          <div className="flex items-center gap-1.5 shrink-0">
                            <button
                              onClick={() => setConfirm({ attachmentId: att.attachment_id, orderId: o.order_id })}
                              className="px-2 py-1 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors"
                            >
                              取り込み確認
                            </button>
                            <button
                              onClick={() => unlink(att.attachment_id)}
                              className="p-1 rounded-md text-muted-foreground hover:text-red-400 hover:bg-red-500/10 transition-colors"
                              aria-label="結び付けを外す"
                              title="結び付けを外す"
                            >
                              <X className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        </div>
                      ) : (
                        <p className="mt-1.5 text-xs text-muted-foreground">
                          {approved ? "承認済みの稼働報告があります" : droppable ? "勤務表をここにドラッグ" : ""}
                        </p>
                      )}
                    </div>
                  );
                })}
              </div>
            ))}
          </section>

          {/* 右: 届いた勤務表 */}
          <section className="bg-card border border-border rounded-lg p-3 space-y-2" aria-label="勤務表">
            <h2 className="text-sm font-semibold text-foreground">
              届いた勤務表（結び付け待ち {unlinkedAtts.length}件）
            </h2>
            {unlinkedAtts.length === 0 && (
              <p className="text-xs text-muted-foreground">結び付け待ちの勤務表はありません。</p>
            )}
            {unlinkedAtts.map((a) => (
              <div
                key={a.attachment_id}
                draggable={a.parsed.ok}
                onDragStart={(e) => { e.dataTransfer.setData("text/plain", String(a.attachment_id)); setDragId(a.attachment_id); }}
                onDragEnd={() => setDragId(null)}
                className={`rounded-md border border-border bg-background px-3 py-2 text-sm ${a.parsed.ok ? "cursor-grab" : "opacity-80"}`}
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <GripVertical className="inline w-3.5 h-3.5 mr-1 text-muted-foreground" />
                    <span className="font-medium text-foreground">{a.parsed.ok ? a.parsed.worker_name || "（氏名なし）" : "読み取れません"}</span>
                    {a.parsed.ok && (
                      <span className="ml-2 text-xs text-muted-foreground">
                        {a.parsed.project_name || "案件名なし"} / {a.parsed.target_month} / {Number(a.parsed.total_hours ?? 0)}h
                      </span>
                    )}
                  </div>
                  <Badge variant="outline" className="text-[10px] shrink-0">
                    {SENDER_LABEL[a.sender_kind]}{a.sender_name ? `: ${a.sender_name}` : ""}
                  </Badge>
                </div>
                <p className="text-xs text-muted-foreground truncate" title={a.filename}>
                  {a.filename}（{a.from_name || a.from_email} ・ {a.received_at.slice(0, 10)}）
                </p>
                {!a.parsed.ok && <p className="text-xs text-red-400 mt-1">{a.parsed.error}</p>}
                {a.superseded && <p className="text-xs text-amber-400 mt-1">同じ人の、より新しい勤務表が届いています</p>}
                {a.parsed.ok && a.suggested_order_id == null && a.candidate_order_ids.length > 1 && (
                  <p className="text-xs text-amber-400 mt-1">同じ人の受注が複数あります。どれか選んでください</p>
                )}
                {a.parsed.ok && a.candidate_order_ids.length === 0 && (
                  a.engineer_known ? (
                    <p className="text-xs text-amber-400 mt-1">
                      {a.parsed.worker_name}さんの{a.parsed.target_month}の受注が登録されていません。受注を登録してください（別の受注に結び付けることもできます）
                    </p>
                  ) : (
                    <p className="text-xs text-amber-400 mt-1">
                      氏名「{a.parsed.worker_name}」が社員と一致しません（表記の違いなど）。左の受注へドラッグするか、下で選んでください
                    </p>
                  )
                )}
                {a.parsed.ok && (
                  <select
                    value=""
                    onChange={(e) => e.target.value && link(a.attachment_id, Number(e.target.value))}
                    className="mt-2 w-full px-2 py-1 text-xs bg-muted border border-border rounded-md text-foreground"
                    aria-label="結び付ける受注を選ぶ"
                  >
                    <option value="">受注を選んで結び付ける…</option>
                    {groups.map(([client, orders]) => (
                      <optgroup key={client} label={client}>
                        {orders.filter((o) => o.can_link && o.timesheet_status !== "APPROVED").map((o) => (
                          <option key={o.order_id} value={o.order_id}>
                            {o.engineer_name} / {o.project_name || "案件名なし"}
                          </option>
                        ))}
                      </optgroup>
                    ))}
                  </select>
                )}
              </div>
            ))}

            {rejectedAtts.length > 0 && (
              <div className="pt-2 space-y-1.5">
                <h3 className="text-xs font-semibold text-red-400">差し戻し済み（再提出待ち {rejectedAtts.length}件）</h3>
                {rejectedAtts.map((a) => (
                  <div key={a.attachment_id} className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs">
                    <div className="flex items-start justify-between gap-2">
                      <span className="text-foreground">
                        {a.parsed.worker_name || a.filename}
                        <span className="ml-2 text-muted-foreground">{a.from_name || a.from_email} ・ {a.filename}</span>
                      </span>
                      <button onClick={() => unreject(a.attachment_id)} className="shrink-0 text-primary hover:underline">
                        差し戻しを取り消す
                      </button>
                    </div>
                    {a.rejected_reason && (
                      <pre className="mt-1 whitespace-pre-wrap text-muted-foreground">{a.rejected_reason}</pre>
                    )}
                  </div>
                ))}
              </div>
            )}

            {importedAtts.length > 0 && (
              <details className="pt-2">
                <summary className="text-xs text-muted-foreground cursor-pointer">取り込み済みの勤務表（{importedAtts.length}件）</summary>
                <ul className="mt-1 space-y-1">
                  {importedAtts.map((a) => (
                    <li key={a.attachment_id} className="text-xs text-muted-foreground">
                      {a.parsed.worker_name || a.filename} / {Number(a.parsed.total_hours ?? 0)}h / {a.sender_name || a.from_name}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </section>
        </div>
      )}

      {confirm && (
        <ImportConfirmDialog
          attachmentId={confirm.attachmentId}
          orderId={confirm.orderId}
          onClose={() => setConfirm(null)}
          onRejected={() => {
            setConfirm(null);
            refresh();
          }}
          onDone={() => {
            setConfirm(null);
            refresh();
            for (const key of ["timesheets", "dashboard", "settlement", "mail-briefs", "received-emails"]) {
              queryClient.invalidateQueries({ queryKey: [key] });
            }
            router.push("/settlement");
          }}
        />
      )}
    </div>
  );
}
