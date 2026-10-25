"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { toast } from "sonner";
import { fetchBillingPreview, confirmBilling } from "@/lib/api";
import type { BillingHint, ForceAction } from "@/lib/types";

type Action = ForceAction["action"];

const ACTION_LABEL: Record<Action, string> = {
  NEXT_MONTH: "翌月の請求書に回す",
  SECOND_INVOICE: "今月分として、2通目の請求書を後で出す",
};

const yen = (n: number) => `¥${n.toLocaleString("ja-JP")}`;

/**
 * 勤務表の承認後に、同じダイアログの中で出す「2 請求書を確認」。
 * 揃っていれば「請求書を確定」、揃っていなければ待つのが基本で、例外のときだけ「待たずに確定」(理由必須)。
 */
export function BillingStep({ hint, onClose }: { hint: BillingHint; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [mode, setMode] = useState<"review" | "force" | "done">("review");
  const [reason, setReason] = useState("");
  const [actions, setActions] = useState<Record<number, Action>>({});
  const [invoiceNo, setInvoiceNo] = useState("");

  const { data, isLoading, error } = useQuery({
    queryKey: ["billing-preview", hint.month, "client", hint.client_id],
    queryFn: () => fetchBillingPreview(hint.month, hint.client_id),
  });
  const invoice = data?.invoices.find((i) => i.key === hint.key);

  const confirmMutation = useMutation({
    mutationFn: confirmBilling,
    onSuccess: (r) => {
      if (!r.success) {
        toast.error(r.error || "確定に失敗しました");
        return;
      }
      setInvoiceNo(r.created.map((c) => c.invoice_no).join("、"));
      setMode("done");
      queryClient.invalidateQueries({ queryKey: ["billing-preview"] });
    },
    onError: (e: Error) => toast.error(`確定に失敗しました: ${e.message}`),
  });

  const missing = invoice?.missing ?? [];
  const allCovered = missing.every((m) => m.client_contract_id in actions);
  const canForce = reason.trim().length > 0 && reason.trim().length <= 500 && allCovered;

  const submitForce = () => {
    if (!invoice || !canForce) return;
    confirmMutation.mutate({
      month: hint.month,
      keys: [invoice.key],
      force: true,
      force_reason: reason.trim(),
      force_actions: missing.map((m) => ({ client_contract_id: m.client_contract_id, action: actions[m.client_contract_id] })),
    });
  };

  const btn = "px-4 py-2 text-sm font-medium rounded-lg border border-border text-foreground hover:bg-muted transition-colors disabled:opacity-40";
  const primary = "px-4 py-2 text-sm font-bold rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity disabled:opacity-40";

  return (
    <div className="flex-1 min-h-0 overflow-auto">
      <div className="w-full max-w-3xl mx-auto mt-6 p-4 space-y-4">
        <ol aria-label="手順" className="flex items-center gap-2 text-xs text-muted-foreground">
          <li className="text-emerald-400">✔ 1 勤務表を承認しました</li>
          <li aria-hidden>→</li>
          <li className="text-primary font-bold">2 請求書を確認</li>
        </ol>

        {isLoading && <p className="text-sm text-muted-foreground">請求書を計算しています...</p>}
        {error && <p className="text-sm text-red-400">請求書を計算できませんでした。請求書の確定の画面から確認してください。</p>}

        {invoice && mode === "review" && (
          <>
            <div className={`border rounded-lg p-3 ${missing.length === 0 ? "border-emerald-500/50 bg-emerald-500/10" : "border-amber-500/50 bg-amber-500/10"}`}>
              <p className="text-sm font-semibold text-foreground">
                {invoice.client_name} {hint.month.replace("-", "年")}月分の請求書{" "}
                <span className={`text-xs ml-1 ${missing.length === 0 ? "text-emerald-400" : "text-amber-400"}`}>
                  {missing.length === 0 ? "全員の勤務表が揃っています" : `あと${missing.length}名の勤務表が未承認`}
                </span>
              </p>
              <p className="text-xs text-muted-foreground mt-1">件名: {invoice.subject}</p>
            </div>
            <table className="w-full text-sm">
              <thead>
                <tr className="text-xs text-muted-foreground text-left">
                  <th className="py-1.5">要員</th><th>案件</th><th>勤務表</th><th className="text-right">金額</th>
                </tr>
              </thead>
              <tbody>
                {invoice.items.map((i) => (
                  <tr key={`${i.client_contract_id}-${i.engineer_id}`} className="border-t border-border">
                    <td className="py-1.5">{i.engineer_name}</td><td>{i.project_name}</td>
                    <td className="text-emerald-400 text-xs">承認済</td><td className="text-right tabular-nums">{yen(i.amount)}</td>
                  </tr>
                ))}
                {missing.map((m) => (
                  <tr key={m.client_contract_id} className="border-t border-border">
                    <td className="py-1.5">{m.engineer_name}</td><td>{m.project_name}</td>
                    <td className="text-amber-400 text-xs">未承認</td><td className="text-right">—</td>
                  </tr>
                ))}
                <tr className="border-t border-border font-semibold">
                  <td className="py-1.5" colSpan={3}>合計(税込)</td><td className="text-right tabular-nums">{yen(invoice.total)}</td>
                </tr>
              </tbody>
            </table>
            <p className="text-xs text-muted-foreground">
              {missing.length === 0
                ? "「請求書を確定」を押すと、請求書が作られます。承認・送付は、請求書の確定の画面で行います。"
                : "全員が揃うのを待つのが基本です。どうしても今月中に請求するときだけ、「待たずに確定(例外)」を使います。"}
            </p>
            {invoice.edi_excluded && (
              <p className="text-xs text-amber-400">この取引先は先方のEDIで請求書を受け取るため、Sophia では確定しません。</p>
            )}
            {/* 左から右へ: あとで確認する → 請求書を確定。例外の「待たずに確定」は右端。ボタンは消さず、使えないときは非活性にする */}
            <div className="flex gap-2">
              <button className={btn} onClick={onClose}>あとで確認する(閉じる)</button>
              <button
                className={primary}
                disabled={!invoice.confirmable || confirmMutation.isPending}
                title={!invoice.confirmable ? "全員の稼働報告が承認済みになると、確定できます" : undefined}
                onClick={() => confirmMutation.mutate({ month: hint.month, keys: [invoice.key], force: false })}
              >
                請求書を確定
              </button>
              <button
                className={`${btn} ml-auto`}
                disabled={missing.length === 0 || invoice.items.length === 0 || invoice.edi_excluded}
                title={missing.length === 0 ? "全員が揃っているので、「請求書を確定」を使います" : undefined}
                onClick={() => setMode("force")}
              >
                待たずに確定(例外)…
              </button>
            </div>
          </>
        )}

        {invoice && mode === "force" && (
          <>
            <div className="border border-amber-500/50 bg-amber-500/10 rounded-lg p-3 text-sm font-semibold text-foreground">
              待たずに確定(例外) <span className="text-xs font-normal text-muted-foreground">— 揃っていない要員がいる状態で請求書を確定します。</span>
            </div>
            <div className="space-y-2">
              <p className="text-xs text-muted-foreground">遅れている要員の扱い(要員ごと)</p>
              {missing.map((m) => (
                <label key={m.client_contract_id} className="flex items-center justify-between gap-3 text-sm">
                  <span>{m.engineer_name}(未承認)</span>
                  <select
                    aria-label={`${m.engineer_name}の扱い`}
                    value={actions[m.client_contract_id] ?? ""}
                    onChange={(e) => setActions((p) => ({ ...p, [m.client_contract_id]: e.target.value as Action }))}
                    className="px-2 py-1.5 text-sm bg-muted border border-border rounded-md text-foreground"
                  >
                    <option value="">選択してください</option>
                    {(Object.keys(ACTION_LABEL) as Action[]).map((a) => (
                      <option key={a} value={a}>{ACTION_LABEL[a]}</option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
            <label className="block text-xs text-muted-foreground">
              理由(必須)
              <textarea
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                maxLength={500}
                rows={3}
                placeholder="例: 先方の締め日が早いため、先に承認済みの分を請求する"
                className="mt-1 w-full px-2 py-1.5 text-sm bg-muted border border-border rounded-md text-foreground"
              />
            </label>
            <p className="text-xs text-muted-foreground">理由は記録に残ります。</p>
            <div className="flex justify-end gap-2">
              <button className={btn} onClick={() => setMode("review")}>戻る</button>
              <button className={primary} disabled={!canForce || confirmMutation.isPending} onClick={submitForce}>待たずに確定</button>
            </div>
          </>
        )}

        {mode === "done" && (
          <>
            <div className="border border-emerald-500/50 bg-emerald-500/10 rounded-lg p-3 text-sm font-semibold text-foreground">
              請求書を確定しました{invoiceNo && `(請求番号 ${invoiceNo})`}
            </div>
            <p className="text-xs text-muted-foreground">次に、請求書の確定の画面で、承認と送付を行います。</p>
            <div className="flex justify-end gap-2">
              <Link href={`/billing?key=${encodeURIComponent(hint.key)}`} className={btn}>請求書の確定の画面で開く →</Link>
              <button className={primary} onClick={onClose}>閉じる</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
