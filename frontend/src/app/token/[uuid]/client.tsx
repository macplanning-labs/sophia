"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useDynamicId } from "@/lib/utils";

type TokenData = {
  kind: "order" | "notice" | "invoice";
  uuid: string;
  partner_name?: string;
  client_name?: string;
  project_name?: string;
  company_name?: string;
  order_id?: string;
  notice_id?: string;
  invoice_no?: string;
  status?: string;
  accepted?: boolean;
  finalized_at?: string | null;
  confirmed_at?: string | null;
  deliverable_text?: string | null;
  payment_condition?: string | null;
  work_location?: string | null;
  target_month?: string;
  notice_date?: string;
  issue_date?: string;
  due_date?: string | null;
  total?: number;
  items?: Array<{
    engineer_name: string;
    base_fee: number;
    effort: number;
    lower_limit_hours: number;
    upper_limit_hours: number;
    price: number;
  }>;
  pdf?: {
    order?: string;
    notice?: string;
    invoice?: string;
    acceptance?: string | null;
  };
  can_accept?: boolean;
  can_upload_timesheet?: boolean;
  /** 版・取り消しの状態(請求書・支払通知書。DEMO-000224) */
  revision_info?: {
    revision: number;
    document_state: "cancelled" | "superseded" | null;
    /** 新しい版を、もう送っているときだけ入る(下書きは見せない) */
    superseded_by: { uuid: string } | null;
    /** 新しい版を作っているが、まだ送っていない */
    superseded_pending: boolean;
  };
  error?: string;
};

/** 差し替え済み・取り消し済みの案内(相手に見える。取り消しの理由は出さない) */
function RevisionNotice({ info }: { info?: TokenData["revision_info"] }) {
  if (!info || !info.document_state) return null;
  if (info.document_state === "cancelled") {
    return (
      <div className="rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">
        この書類は取り消されました。
      </div>
    );
  }
  return (
    <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-400 space-y-2">
      <div>この書類は、新しい版に差し替えられました。</div>
      {info.superseded_by ? (
        <a
          href={`/token/${info.superseded_by.uuid}`}
          className="inline-flex px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium"
        >
          新しい版を見る
        </a>
      ) : (
        <div>新しい版は、準備中です。届き次第、そちらをご確認ください。</div>
      )}
    </div>
  );
}

function formatYen(n: number | undefined) {
  if (n == null) return "—";
  return `¥${Math.round(n).toLocaleString("ja-JP")}`;
}

export default function TokenPage() {
  const uuid = useDynamicId();
  const fileRef = useRef<HTMLInputElement>(null);

  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<TokenData | null>(null);
  const [error, setError] = useState("");
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!uuid || uuid === "_") return;
    setLoading(true);
    setError("");
    try {
      const res = await fetch(`/api/v1/token/${uuid}`, { credentials: "include" });
      const json = await res.json();
      if (!res.ok || json.error) {
        setError(json.error || "リンクが見つかりません");
        setData(null);
      } else {
        setData(json as TokenData);
      }
    } catch {
      setError("サーバーに接続できません");
      setData(null);
    } finally {
      setLoading(false);
    }
  }, [uuid]);

  useEffect(() => {
    if (uuid) void load();
  }, [uuid, load]);

  const handleAccept = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const res = await fetch(`/api/v1/token/${uuid}/accept`, {
        method: "POST",
        credentials: "include",
      });
      const json = await res.json();
      if (json.success) {
        const successMessage = data?.kind === "invoice"
          ? "受領確認しました。ありがとうございました。"
          : "承諾しました。ありがとうございました。";
        setMessage({ ok: true, text: successMessage });
        await load();
      } else {
        setMessage({ ok: false, text: json.error || "承諾に失敗しました" });
      }
    } catch {
      setMessage({ ok: false, text: "サーバーに接続できません" });
    } finally {
      setBusy(false);
    }
  };

  const handleUpload = async (e: React.FormEvent) => {
    e.preventDefault();
    const file = fileRef.current?.files?.[0];
    if (!file) {
      setMessage({ ok: false, text: "ファイルを選択してください" });
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const form = new FormData();
      form.append("file", file);
      const res = await fetch(`/api/v1/token/${uuid}/timesheet`, {
        method: "POST",
        credentials: "include",
        body: form,
      });
      const json = await res.json();
      if (json.success) {
        setMessage({ ok: true, text: json.message || "受け付けました" });
        if (fileRef.current) fileRef.current.value = "";
      } else {
        setMessage({ ok: false, text: json.error || "アップロードに失敗しました" });
      }
    } catch {
      setMessage({ ok: false, text: "サーバーに接続できません" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="min-h-screen bg-background text-foreground">
      <div className="max-w-2xl mx-auto px-4 py-10">
        <div className="mb-8">
          <h1 className="text-2xl font-bold bg-gradient-to-r from-sky-400 to-cyan-400 bg-clip-text text-transparent">
            Sophia
          </h1>
          <p className="text-muted-foreground text-sm mt-1">書類確認</p>
        </div>

        {loading && <p className="text-muted-foreground text-sm">読み込み中…</p>}

        {!loading && error && (
          <div className="border border-red-500/30 bg-red-500/10 rounded-xl p-6 text-red-400 text-sm">
            {error}
          </div>
        )}

        {!loading && data && (
          <div className="space-y-6">
            <p className="text-sm">
              {(data.partner_name || data.client_name || "")} 様
            </p>

            {message && (
              <div
                className={`rounded-lg border p-3 text-sm ${
                  message.ok
                    ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-400"
                    : "border-red-500/30 bg-red-500/10 text-red-400"
                }`}
              >
                {message.text}
              </div>
            )}

            {data.kind === "order" && (
              <>
                {data.accepted ? (
                  <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm text-emerald-400">
                    この注文書は承諾済みです
                    {data.finalized_at ? `（${data.finalized_at}）` : ""}。
                  </div>
                ) : (
                  <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-400">
                    まだ承諾されていません。内容をご確認の上、承諾をお願いします。
                  </div>
                )}

                <div className="flex flex-wrap gap-2">
                  {data.pdf?.order && (
                    <a
                      href={data.pdf.order}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm hover:bg-accent/50"
                    >
                      注文書PDFを見る
                    </a>
                  )}
                  {data.pdf?.acceptance ? (
                    <a
                      href={data.pdf.acceptance}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm hover:bg-accent/50"
                    >
                      請書PDFを見る
                    </a>
                  ) : (
                    <span
                      aria-disabled="true"
                      title="承諾すると、請書を見られます"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm opacity-40 cursor-not-allowed"
                    >
                      請書PDFを見る
                    </span>
                  )}
                  {/* 承諾のボタンは消さず、承諾済みのあとは非活性にする(押した印) */}
                  <button
                    type="button"
                    disabled={busy || !data.can_accept}
                    onClick={handleAccept}
                    className="inline-flex px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50"
                  >
                    {data.accepted ? "承諾済み" : "内容を確認し、承諾する"}
                  </button>
                </div>

                <section>
                  <h2 className="text-sm font-semibold mb-2">注文情報</h2>
                  <dl className="text-sm space-y-1.5 border border-border rounded-lg p-4">
                    <Row label="注文番号" value={data.order_id} />
                    <Row label="プロジェクト" value={data.project_name} />
                    <Row label="成果物" value={data.deliverable_text} />
                    <Row label="支払条件" value={data.payment_condition} />
                    <Row label="作業場所" value={data.work_location} />
                  </dl>
                </section>

                {data.items && data.items.length > 0 && (
                  <section>
                    <h2 className="text-sm font-semibold mb-2">明細</h2>
                    <div className="overflow-x-auto border border-border rounded-lg">
                      <table className="w-full text-sm">
                        <thead className="bg-muted/40 text-muted-foreground">
                          <tr>
                            <th className="text-left px-3 py-2 font-medium">エンジニア</th>
                            <th className="text-right px-3 py-2 font-medium">単価</th>
                            <th className="text-right px-3 py-2 font-medium">人月</th>
                            <th className="text-right px-3 py-2 font-medium">金額</th>
                          </tr>
                        </thead>
                        <tbody>
                          {data.items.map((item, i) => (
                            <tr key={i} className="border-t border-border">
                              <td className="px-3 py-2">{item.engineer_name}</td>
                              <td className="px-3 py-2 text-right">{formatYen(item.base_fee)}</td>
                              <td className="px-3 py-2 text-right">{item.effort}</td>
                              <td className="px-3 py-2 text-right">{formatYen(item.price)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </section>
                )}

                {/* 稼働報告書の提出は消さず、承諾前・作業月の開始前は非活性にする */}
                  <section className="border border-border rounded-lg p-4 space-y-3">
                    <h2 className="text-sm font-semibold">稼働報告書の提出</h2>
                    <p className="text-muted-foreground text-xs">
                      {data.can_upload_timesheet
                        ? "作業月分の稼働報告書（Excel/PDF）をアップロードしてください。"
                        : "承諾後、作業月の開始日から、提出できます。"}
                    </p>
                    <form onSubmit={handleUpload} className="space-y-3">
                      <input
                        ref={fileRef}
                        type="file"
                        accept=".xlsx,.xls,.pdf"
                        disabled={!data.can_upload_timesheet}
                        className="block w-full text-sm text-muted-foreground file:mr-3 file:py-1.5 file:px-3 file:rounded-lg file:border-0 file:bg-primary/10 file:text-primary"
                      />
                      <button
                        type="submit"
                        disabled={busy || !data.can_upload_timesheet}
                        className="px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50"
                      >
                        稼働報告書を提出する
                      </button>
                    </form>
                  </section>
              </>
            )}

            {data.kind === "notice" && (
              <>
                <RevisionNotice info={data.revision_info} />
                {data.accepted ? (
                  <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm text-emerald-400">
                    この支払通知書・請求書は承諾済みです
                    {data.confirmed_at ? `（${data.confirmed_at}）` : ""}。
                  </div>
                ) : (
                  !data.revision_info?.document_state && (
                    <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-400">
                      まだ承諾されていません。支払通知書と請求書をご確認の上、承諾をお願いします。
                    </div>
                  )
                )}

                <div className="flex flex-wrap gap-2">
                  {data.pdf?.notice && (
                    <a
                      href={data.pdf.notice}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm hover:bg-accent/50"
                    >
                      支払通知書PDFを見る
                    </a>
                  )}
                  {data.pdf?.invoice && (
                    <a
                      href={data.pdf.invoice}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm hover:bg-accent/50"
                    >
                      請求書PDFを見る
                    </a>
                  )}
                  {/* 承諾のボタンは消さず、使えないときは非活性にする(承諾済み・差し替え済み・取り消し済み) */}
                  <button
                    type="button"
                    disabled={busy || !data.can_accept}
                    onClick={handleAccept}
                    className="inline-flex px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50"
                  >
                    {data.accepted ? "承諾済み" : "内容を確認し、承諾する"}
                  </button>
                </div>

                <dl className="text-sm space-y-1.5 border border-border rounded-lg p-4">
                  <Row label="通知書番号" value={data.notice_id} />
                  <Row label="プロジェクト" value={data.project_name} />
                  <Row label="対象月" value={data.target_month} />
                  <Row label="通知日" value={data.notice_date} />
                  <Row label="税込合計" value={formatYen(data.total)} />
                </dl>
                <p className="text-xs text-muted-foreground">
                  「請求書」は、貴社に代わり当社が作成した請求書です。承諾により貴社発行の請求書として受理します。
                </p>
              </>
            )}

            {data.kind === "invoice" && (
              <>
                <RevisionNotice info={data.revision_info} />
                {data.accepted ? (
                  <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm text-emerald-400">
                    受領確認済みです
                    {data.confirmed_at ? `（${data.confirmed_at}）` : ""}。ご対応ありがとうございました。
                  </div>
                ) : (
                  !data.revision_info?.document_state && (
                    <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-400">
                      まだ受領のご確認をいただいておりません。内容をご確認の上、受領確認をお願いします。
                    </div>
                  )
                )}

                <div className="flex flex-wrap gap-2">
                  {data.pdf?.invoice && (
                    <a
                      href={data.pdf.invoice}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex px-3 py-2 rounded-lg border border-border text-sm hover:bg-accent/50"
                    >
                      請求書PDFを見る
                    </a>
                  )}
                  {/* 受領確認のボタンは消さず、使えないときは非活性にする(確認済み・差し替え済み・取り消し済み) */}
                  <button
                    type="button"
                    disabled={busy || !data.can_accept}
                    onClick={handleAccept}
                    className="inline-flex px-3 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium disabled:opacity-50"
                  >
                    {data.accepted ? "受領確認済み" : "内容を確認しました（受領確認）"}
                  </button>
                </div>

                <dl className="text-sm space-y-1.5 border border-border rounded-lg p-4">
                  <Row label="請求書番号" value={data.invoice_no} />
                  <Row label="対象月" value={data.target_month} />
                  <Row label="発行日" value={data.issue_date} />
                  <Row label="支払期限" value={data.due_date || undefined} />
                  <Row label="税込合計" value={formatYen(data.total)} />
                </dl>
              </>
            )}

            {data.company_name && (
              <p className="text-xs text-muted-foreground pt-2">{data.company_name}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function Row({ label, value }: { label: string; value?: string | null }) {
  if (!value) return null;
  return (
    <div className="flex gap-3">
      <dt className="w-28 shrink-0 text-muted-foreground">{label}</dt>
      <dd className="flex-1">{value}</dd>
    </div>
  );
}
