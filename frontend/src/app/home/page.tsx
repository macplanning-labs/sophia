"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { toast } from "sonner";
import { useCurrentMonth } from "@/lib/MonthContext";
import {
  fetchStatusMatrix,
  fetchInvoices,
  fetchReceivedEmails,
  fetchBillingPreview,
  confirmBilling,
  approveInvoice,
  rejectInvoice,
  apiPost,
} from "@/lib/api";
import { buildHomeCards, type HomeCard } from "@/lib/home-cards";
import { PageHeader } from "@/components/ui/page-header";
import { ConfirmSheet, type ConfirmFact } from "@/components/ui/confirm-sheet";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { StatusRow } from "@/lib/types";

const yen = (n: number | null | undefined) => `¥${(n ?? 0).toLocaleString("ja-JP")}`;

interface Pending {
  title: string;
  confirmLabel: string;
  danger?: boolean;
  facts: ConfirmFact[];
  run: () => Promise<unknown>;
}

const TONE_CLASS = {
  ok: "border-emerald-500/40 text-emerald-400",
  warn: "border-amber-500/40 text-amber-400",
  bad: "border-rose-500/40 text-rose-400",
  na: "border-border text-muted-foreground",
} as const;

function Badge({ tone, label }: { tone: keyof typeof TONE_CLASS; label: string }) {
  return <span className={`inline-block rounded-full border px-2 py-px text-[11px] whitespace-nowrap ${TONE_CLASS[tone]}`}>{label}</span>;
}

const linkCls = "text-xs text-primary hover:underline whitespace-nowrap";
const btnCls = "rounded-md border border-border px-2.5 py-1 text-xs text-foreground hover:bg-muted";
const btnPrimary = "rounded-md bg-primary px-2.5 py-1 text-xs font-bold text-primary-foreground hover:opacity-90";
const btnDanger = "rounded-md border border-rose-500/50 px-2.5 py-1 text-xs text-rose-400 hover:bg-rose-500/10";

/** ホーム(サイドバー > ホーム > 要対応)。上のカードを選ぶと、下に同じ範囲の一覧と、この画面でできる操作が出る */
export default function HomePage() {
  const { month } = useCurrentMonth();
  const qc = useQueryClient();
  const matrix = useQuery({ queryKey: ["status-matrix", month], queryFn: () => fetchStatusMatrix(month) });
  const invoices = useQuery({ queryKey: ["invoices"], queryFn: fetchInvoices });
  // 受信メールの画面の初期表示(要確認 + 取引先ドメインのみ)と同じ条件で数える。条件が違うと件数がずれる
  const mails = useQuery({ queryKey: ["received-emails", true, true], queryFn: () => fetchReceivedEmails(true, true) });
  const preview = useQuery({ queryKey: ["billing-preview", month], queryFn: () => fetchBillingPreview(month) });

  const [selected, setSelected] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);

  const loading = matrix.isLoading || invoices.isLoading || mails.isLoading || preview.isLoading;
  const failed = matrix.error || invoices.error || mails.error || preview.error;

  const confirmable = (preview.data?.invoices ?? []).filter((i) => i.confirmable && !i.edi_excluded);
  const pendingInvoices = (invoices.data ?? []).filter(
    (i) => i.status === "PENDING_APPROVAL" && (i.target_month ?? "").startsWith(month),
  );
  const mailRows = mails.data?.emails ?? [];
  const cards = buildHomeCards(matrix.data, invoices.data ?? [], mailRows.length, month, confirmable.length);
  const current: HomeCard | undefined = cards.find((c) => c.key === selected) ?? cards[0];
  const rows = matrix.data?.rows ?? [];

  const refresh = () => {
    for (const k of ["status-matrix", "invoices", "received-emails", "billing-preview", "dashboard"]) {
      qc.invalidateQueries({ queryKey: [k] });
    }
  };
  const exec = useMutation({
    mutationFn: async (p: Pending) => p.run(),
    onSuccess: () => {
      toast.success("実行しました");
      setPending(null);
      refresh();
    },
    onError: (e: Error) => toast.error(`失敗しました: ${e.message}`),
  });

  const ask = (p: Pending) => setPending(p);

  function matrixTable(pick: (r: StatusRow) => boolean, cell: (r: StatusRow) => { tone: keyof typeof TONE_CLASS; label: string }, openLabel: string, openHref: string) {
    const list = rows.filter(pick);
    return (
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>要員</TableHead>
            <TableHead>取引先 / 案件</TableHead>
            <TableHead>状況</TableHead>
            <TableHead className="text-right">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {list.map((r) => (
            <TableRow key={`${r.engineer_id}-${r.project_id}`}>
              <TableCell className="text-sm">{r.engineer_name}</TableCell>
              <TableCell className="text-sm">{r.client_name} / {r.project_name}</TableCell>
              <TableCell><Badge {...cell(r)} /></TableCell>
              <TableCell className="text-right"><Link href={openHref} className={linkCls}>{openLabel}</Link></TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    );
  }

  function panelBody(c: HomeCard) {
    switch (c.key) {
      case "ts-returned":
        return matrixTable((r) => r.timesheet.label === "差し戻し", (r) => r.timesheet, "勤務表の画面で開く →", c.href);
      case "ts-review":
        return matrixTable((r) => r.timesheet.label === "確認待ち", (r) => r.timesheet, "勤務表の画面で開く →", c.href);
      case "notice":
        return matrixTable((r) => r.notice.label === "未作成", (r) => r.notice, "月次確定の画面で開く →", "/settlement");
      case "po":
        return matrixTable((r) => r.purchase_order.label === "未送付", (r) => r.purchase_order, "発注契約の画面で開く →", "/partner-contracts");
      case "ro":
        return matrixTable((r) => r.received_order.label === "注文書なし", (r) => r.received_order, "受注契約の画面で開く →", "/client-contracts");
      case "inv-confirm":
        return (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>取引先 / 案件</TableHead>
                <TableHead className="text-right">明細</TableHead>
                <TableHead className="text-right">合計(税込)</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {confirmable.map((i) => (
                <TableRow key={i.key}>
                  <TableCell className="text-sm">{i.client_name}<div className="text-xs text-muted-foreground">{i.subject}</div></TableCell>
                  <TableCell className="text-right">{i.items.length}</TableCell>
                  <TableCell className="text-right">{yen(i.total)}</TableCell>
                  <TableCell className="text-right">
                    <button
                      className={btnPrimary}
                      onClick={() =>
                        ask({
                          title: "請求書を確定",
                          confirmLabel: "確定",
                          facts: [
                            { label: "取引先", value: i.client_name },
                            { label: "件名", value: i.subject },
                            { label: "合計(税込)", value: yen(i.total) },
                          ],
                          run: async () => {
                            const r = await confirmBilling({ month, keys: [i.key], force: false });
                            if (!r.success) throw new Error(r.error || "確定できませんでした");
                          },
                        })
                      }
                    >
                      請求書を確定
                    </button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        );
      case "inv-approve":
        return (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>請求書番号</TableHead>
                <TableHead>クライアント</TableHead>
                <TableHead>件名</TableHead>
                <TableHead className="text-right">合計(税込)</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {pendingInvoices.map((i) => {
                const facts: ConfirmFact[] = [
                  { label: "請求書番号", value: i.invoice_id },
                  { label: "クライアント", value: i.client_name },
                  { label: "合計(税込)", value: yen(i.total_amount) },
                ];
                return (
                  <TableRow key={i.id}>
                    <TableCell><Link href={`/invoices/${i.id}`} className="text-primary hover:underline">{i.invoice_id}</Link></TableCell>
                    <TableCell>{i.client_name}</TableCell>
                    <TableCell className="text-sm">{i.subject}</TableCell>
                    <TableCell className="text-right">{yen(i.total_amount)}</TableCell>
                    <TableCell>
                      <div className="flex justify-between gap-6">
                        <button className={btnPrimary} onClick={() => ask({ title: "請求書を承認", confirmLabel: "承認", facts, run: () => approveInvoice(String(i.id)) })}>承認</button>
                        <button className={btnDanger} onClick={() => ask({ title: "請求書を差し戻し", confirmLabel: "差し戻し", danger: true, facts, run: () => rejectInvoice(String(i.id)) })}>差し戻し</button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        );
      case "mail":
        return (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>差出人</TableHead>
                <TableHead>件名</TableHead>
                <TableHead>受信日</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {mailRows.map((m) => (
                <TableRow key={m.id}>
                  <TableCell className="text-sm">{m.from_name || m.from_email}</TableCell>
                  <TableCell className="text-sm">{m.subject}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">{new Date(m.received_at).toLocaleDateString("ja-JP")}</TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-2">
                      <button
                        className={btnPrimary}
                        onClick={() =>
                          ask({
                            title: "要確認を解除",
                            confirmLabel: "解除",
                            facts: [{ label: "差出人", value: m.from_name || m.from_email }, { label: "件名", value: m.subject }],
                            run: async () => {
                              const r = await apiPost<{ success: boolean; error?: string }>(`/api/v1/received-emails/${m.id}/resolve`, {});
                              if (!r.success) throw new Error(r.error || "解除できませんでした");
                            },
                          })
                        }
                      >
                        要確認を解除
                      </button>
                      <Link href={`/received-emails?open=${m.id}`} className={linkCls}>メールを開く →</Link>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        );
    }
  }

  const openHref: Record<string, { label: string; href: string }> = {
    "ts-returned": { label: "勤務表の画面で開く →", href: current?.href ?? "" },
    "ts-review": { label: "勤務表の画面で開く →", href: current?.href ?? "" },
    "inv-confirm": { label: "請求書の確定の画面で開く →", href: "/billing" },
    "inv-approve": { label: "請求書の画面で開く →", href: "/invoices" },
    notice: { label: "月次確定の画面で開く →", href: "/settlement" },
    po: { label: "発注契約の画面で開く →", href: "/partner-contracts" },
    ro: { label: "受注契約の画面で開く →", href: "/client-contracts" },
    mail: { label: "受信メールの画面で開く →", href: "/received-emails" },
  };

  return (
    <div className="space-y-4 p-6">
      <PageHeader />

      {loading && <p className="text-sm text-muted-foreground">読み込み中...</p>}
      {failed && <p className="text-sm text-red-400">一部の件数を取得できませんでした。画面を更新してください。</p>}

      {!loading && !failed && cards.length === 0 && (
        <div className="rounded-xl border border-emerald-500/40 bg-emerald-500/10 p-5 text-center font-bold text-emerald-400">
          ✔ いま手当てが必要なものは、ありません
        </div>
      )}

      <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-3">
        {cards.map((c) => {
          const sel = current?.key === c.key;
          return (
            <button
              key={c.key}
              type="button"
              aria-pressed={sel}
              onClick={() => setSelected(c.key)}
              className={`rounded-xl border p-3 text-left transition-colors ${
                sel ? "border-primary bg-primary/10 ring-1 ring-primary" : "border-border bg-card hover:border-primary/60"
              }`}
            >
              <div className="text-xs text-muted-foreground">{c.title}</div>
              <div className={`text-2xl font-bold ${c.tone === "bad" ? "text-rose-400" : "text-amber-400"}`}>
                {c.count}
                <span className="ml-1 text-xs font-normal">件</span>
              </div>
              <div className="text-[11px] text-muted-foreground">{c.description}</div>
            </button>
          );
        })}
      </div>

      {current && (
        <section className="rounded-xl border border-border bg-card p-4" aria-label="一覧">
          <div className="mb-2 flex items-center justify-between gap-3">
            <b className="text-sm text-foreground">
              {current.title} — {current.description}({current.count}件)
            </b>
            <Link href={openHref[current.key].href} className={linkCls}>{openHref[current.key].label}</Link>
          </div>
          {panelBody(current)}
        </section>
      )}

      <ConfirmSheet
        open={!!pending}
        title={pending?.title ?? ""}
        facts={pending?.facts}
        confirmLabel={pending?.confirmLabel}
        variant={pending?.danger ? "danger" : "default"}
        loading={exec.isPending}
        onConfirm={() => pending && exec.mutate(pending)}
        onCancel={() => setPending(null)}
      />
    </div>
  );
}
