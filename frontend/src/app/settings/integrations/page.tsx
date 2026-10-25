"use client";

// 外部連携(サイドバー「設定」> タブ「外部連携」。管理者のみ)。DEMO-000215、承認済みモック(2026-10-12)
// 見た目は「APIキー」画面と同じ部品(bg-card のカード、text-sm text-muted-foreground の見出し)にそろえる。

import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { Bell, FileText, Inbox, Mail, Server, History } from "lucide-react";
import {
  addIntegrationTarget,
  fetchIntegrationState,
  runIntegrationCheck,
  sendIntegrationTest,
  updateIntegrationTarget,
  type IntegrationState,
  type IntegrationTarget,
  type IntegrationTargetInput,
} from "@/lib/api";
import { PageHeader } from "@/components/ui/page-header";
import { Button } from "@/components/ui/button";
import { NotifyTargetDialog, StoreTargetDialog } from "@/components/integrations/target-dialogs";

type DialogState = { kind: "notify" | "store"; target: IntegrationTarget | null } | null;

function Pill({ kind, children }: { kind: "ok" | "ng" | "off"; children: React.ReactNode }) {
  const cls =
    kind === "ok"
      ? "text-emerald-400 border-emerald-500/40 bg-emerald-500/10"
      : kind === "ng"
        ? "text-red-400 border-red-500/40 bg-red-500/10"
        : "text-muted-foreground border-border";
  return <span className={`ml-auto rounded-full border px-2 py-0.5 text-xs ${cls}`}>{children}</span>;
}

function Result({ label, at, ok, okText, ngText, extra }: { label: string; at: string | null; ok: boolean | null; okText: string; ngText: string; extra?: string | null }) {
  if (!at) return <span>{label}: まだありません</span>;
  return (
    <span>
      {label}: <span className={ok ? "text-emerald-400" : "text-red-400"}>{ok ? okText : ngText}</span> {at}
      {extra ? `(${extra})` : ""}
    </span>
  );
}

/** 宛先は名前で出し、ID は小さく添える。名前が分からないときは「未登録」と出す */
function Destination({ t }: { t: IntegrationTarget }) {
  const kind = t.provider === "gdrive" ? "フォルダ" : "スペース";
  return (
    <>
      {t.destination_name ? (
        <span className="text-foreground">{t.destination_name}</span>
      ) : (
        <span className="text-amber-400">{kind}名が未登録{t.source === "env" ? "(画面の設定に移すと登録できます)" : ""}</span>
      )}
      {t.destination && <span className="ml-1 text-xs text-muted-foreground">({kind} ID: {t.destination})</span>}
    </>
  );
}

function TargetRow({
  t,
  onCheck,
  onTest,
  onEdit,
  busy,
  showDelivery,
  children,
}: {
  t: IntegrationTarget;
  onCheck: () => void;
  onTest?: () => void;
  onEdit: () => void;
  busy: boolean;
  showDelivery?: boolean;
  children?: React.ReactNode;
}) {
  const checkNg = t.check.ok === false || !t.usable;
  return (
    <div className={`flex flex-col gap-3 rounded-lg border p-4 sm:flex-row sm:items-center sm:justify-between ${checkNg ? "border-red-500/50 border-dashed" : "border-border"}`}>
      <div className="min-w-0">
        <p className="font-medium">{t.label}</p>
        <p className="text-sm text-muted-foreground">
          つなぎ先: <span className="text-foreground">{t.provider_label}</span>
          {" ・ "}宛先: <Destination t={t} />
          {t.source === "env" && <span className="ml-2 text-xs text-amber-400">サーバの設定ファイルから読んでいます</span>}
        </p>
        {!t.usable && t.problem && <p className="mt-1 text-sm text-red-400">使えません: {t.problem}</p>}
        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <Result label="点検" at={t.check.at} ok={t.check.ok} okText="正常" ngText="異常" />
          {showDelivery && <Result label="テスト送信" at={t.test.at} ok={t.test.ok} okText="届いた" ngText="届かない" extra={t.test.by} />}
          {showDelivery && <Result label="実際の送信" at={t.delivery.at} ok={t.delivery.ok} okText="届いた" ngText="届かない" />}
          {children}
        </div>
        {t.check.ok === false && Array.isArray(t.check.detail) && (
          <ul className="mt-1 text-xs text-red-300">
            {(t.check.detail as string[]).filter((d) => d.startsWith("NG")).map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
        )}
      </div>
      <div className="flex shrink-0 gap-2">
        <Button variant="outline" size="sm" onClick={onCheck} disabled={busy}>点検する</Button>
        {onTest && (
          <Button variant="outline" size="sm" onClick={onTest} disabled={busy || !t.usable}>テスト送信</Button>
        )}
        <Button variant="outline" size="sm" onClick={onEdit} disabled={busy}>
          {t.source === "env" ? "画面の設定に移す" : "変更"}
        </Button>
      </div>
    </div>
  );
}

const QUERY_KEY = ["integration-settings"];

export default function IntegrationsPage() {
  const qc = useQueryClient();
  const { data: state, error: loadError } = useQuery({ queryKey: QUERY_KEY, queryFn: fetchIntegrationState });
  const setState = (s: IntegrationState) => {
    qc.setQueryData(QUERY_KEY, s);
    // 画面の上部の赤い帯も、直した結果にそろえる
    void qc.invalidateQueries({ queryKey: ["integration-alert"] });
  };
  const [busy, setBusy] = useState(false);
  const [actionError, setError] = useState<string | null>(null);
  const error = actionError ?? (loadError ? (loadError instanceof Error ? loadError.message : "読み込みに失敗しました") : null);
  const [notice, setNotice] = useState<string | null>(null);
  const [dialog, setDialog] = useState<DialogState>(null);

  const run = async (fn: () => Promise<IntegrationState>, done?: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      setState(await fn());
      if (done) setNotice(done);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "失敗しました");
      return false;
    } finally {
      setBusy(false);
    }
  };

  const check = () => run(runIntegrationCheck, "点検しました");

  const test = async (t: IntegrationTarget) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const r = await sendIntegrationTest(t.id);
      setState(r.state);
      if (r.ok) setNotice(`「${t.label}」にテストの通知を送りました。届いているか確かめてください。`);
      else setError(`テスト送信に失敗しました: ${r.error ?? ""}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "テスト送信に失敗しました");
    } finally {
      setBusy(false);
    }
  };

  const save = async (id: number | null, input: IntegrationTargetInput) => {
    const ok = await run(() => (id === null ? addIntegrationTarget(input) : updateIntegrationTarget(id, input)), "保存して点検しました");
    if (ok) setDialog(null);
  };

  if (!state) {
    return (
      <div className="p-6 space-y-6 max-w-4xl">
        <PageHeader />
        {error ? (
          <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">{error}</div>
        ) : (
          <p className="text-sm text-muted-foreground">読み込み中…</p>
        )}
      </div>
    );
  }

  const storeTarget = state.store.targets[0] ?? null;
  const notifyTargets = state.notify.targets;

  return (
    <div className="p-6 space-y-6 max-w-4xl">
      <PageHeader />

      {error && <div className="rounded-lg border border-red-500/40 bg-red-500/10 p-3 text-sm text-red-300">{error}</div>}
      {notice && <div className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-3 text-sm text-emerald-200">{notice}</div>}

      {/* 全体の状態 */}
      {state.overall ? (
        <div
          data-testid="integration-overall"
          className={`rounded-lg border p-4 text-sm ${state.overall.ok ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-200" : "border-red-500/40 bg-red-500/10 text-red-200"}`}
        >
          <div className="flex items-start justify-between gap-3">
            <div>
              {state.overall.ok ? "✅ 必須の連携は、すべて使えます。" : `🔥 使えない連携が ${state.overall.problems.length} 件あります。下の赤い枠を確かめてください。`}
              <p className="mt-1 text-xs text-muted-foreground">
                最後の点検: {state.overall.at}
                {!state.overall.ok && "(この状態は「運用監視」にも通知しています)"}
              </p>
            </div>
            <Button variant="outline" size="sm" onClick={check} disabled={busy}>すべて点検する</Button>
          </div>
        </div>
      ) : (
        <div className="rounded-lg border border-border bg-card p-4 text-sm text-muted-foreground flex items-center justify-between">
          まだ点検していません。
          <Button variant="outline" size="sm" onClick={check} disabled={busy}>すべて点検する</Button>
        </div>
      )}

      {/* 異常の通知 */}
      <section className="bg-card border border-border rounded-lg p-5 space-y-2">
        <div className="flex items-center gap-2">
          <Bell className="w-4 h-4 text-amber-400" />
          <h2 className="text-sm text-muted-foreground">異常の通知</h2>
          {notifyTargets.length === 0 ? <Pill kind="ng">未設定</Pill> : notifyTargets.every((t) => t.usable && t.check.ok !== false) ? <Pill kind="ok">正常</Pill> : <Pill kind="ng">異常</Pill>}
        </div>
        <p className="text-xs text-muted-foreground">
          メール取込の障害、期限超過、怪しいアクセス、点検の異常を知らせます。送り先は複数にできます(全部へ送ります)。
        </p>
        {notifyTargets.map((t) => (
          <TargetRow key={`${t.source}-${t.id ?? t.label}`} t={t} busy={busy} showDelivery onCheck={check} onTest={() => test(t)} onEdit={() => setDialog({ kind: "notify", target: t })} />
        ))}
        {notifyTargets.length === 0 && (
          <div className="rounded-lg border border-dashed border-red-500/50 p-4 text-sm text-red-300">送り先がありません。異常が起きても誰にも届きません。</div>
        )}
        <button
          type="button"
          onClick={() => setDialog({ kind: "notify", target: null })}
          disabled={busy}
          className="w-full rounded-lg border border-dashed border-border p-2.5 text-sm text-muted-foreground hover:bg-muted disabled:opacity-50"
        >
          + 送り先を足す
        </button>
      </section>

      {/* 発行した書類の保存 */}
      <section className="bg-card border border-border rounded-lg p-5 space-y-2">
        <div className="flex items-center gap-2">
          <FileText className="w-4 h-4 text-violet-400" />
          <h2 className="text-sm text-muted-foreground">発行した書類の保存</h2>
          {!storeTarget ? <Pill kind="ng">未設定</Pill> : storeTarget.usable && storeTarget.check.ok !== false ? <Pill kind="ok">正常</Pill> : <Pill kind="ng">使えない</Pill>}
        </div>
        <p className="text-xs text-muted-foreground">送った注文書・支払通知書・請求書と、注文請書の PDF を保存します(正本は Sophia の中。ここは写し)。</p>
        {storeTarget ? (
          <TargetRow t={storeTarget} busy={busy} onCheck={check} onEdit={() => setDialog({ kind: "store", target: storeTarget })}>
            <span>
              保存待ち: <span className={state.store.pending_count > 0 ? "text-amber-400" : "text-foreground"}>{state.store.pending_count} 件</span>
            </span>
            <span>最後の保存: {state.store.last_stored ? `${state.store.last_stored.at} ${state.store.last_stored.name}` : "まだありません"}</span>
          </TargetRow>
        ) : (
          <div className="rounded-lg border border-dashed border-red-500/50 p-4 text-sm">
            <p className="text-red-300">保存先が設定されていません。PDF は Sophia の中に残り、保存待ちになります(保存待ち: {state.store.pending_count} 件)。</p>
            <Button className="mt-2" size="sm" onClick={() => setDialog({ kind: "store", target: null })} disabled={busy}>保存先を設定する</Button>
          </div>
        )}
      </section>

      {/* 受信メールの添付の保存 */}
      <section className="bg-card border border-border rounded-lg p-5 space-y-2">
        <div className="flex items-center gap-2">
          <Inbox className="w-4 h-4 text-sky-400" />
          <h2 className="text-sm text-muted-foreground">受信メールの添付の保存</h2>
          {!storeTarget ? <Pill kind="ng">未設定</Pill> : storeTarget.usable && storeTarget.check.ok !== false ? <Pill kind="ok">正常</Pill> : <Pill kind="ng">使えない</Pill>}
        </div>
        <p className="text-xs text-muted-foreground">取引先から届いた注文書・勤務表などを保存します。「発行した書類の保存」と同じ設定を使います。</p>
        {storeTarget && (
          <div className="flex flex-col gap-3 rounded-lg border border-border p-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <p className="font-medium">{storeTarget.label}</p>
              <p className="text-sm text-muted-foreground">
                つなぎ先: <span className="text-foreground">{storeTarget.provider_label}</span> ・ 宛先: <Destination t={storeTarget} />
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button variant="outline" size="sm" onClick={check} disabled={busy}>点検する</Button>
              <Button variant="outline" size="sm" onClick={() => setDialog({ kind: "store", target: storeTarget })} disabled={busy}>
                {storeTarget.source === "env" ? "画面の設定に移す" : "変更"}
              </Button>
            </div>
          </div>
        )}
      </section>

      {/* メール */}
      <section className="bg-card border border-border rounded-lg p-5 space-y-2">
        <div className="flex items-center gap-2">
          <Mail className="w-4 h-4 text-emerald-400" />
          <h2 className="text-sm text-muted-foreground">メールの送信・受信</h2>
          {state.mail.from ? <Pill kind="ok">設定あり</Pill> : <Pill kind="ng">未設定</Pill>}
        </div>
        <p className="text-xs text-muted-foreground">メールの設定は、これまでどおり「メール」の画面で変えます。ここでは状態だけを表示します。</p>
        <div className="flex flex-col gap-3 rounded-lg border border-border p-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <p className="font-medium">送信(Gmail)</p>
            <p className="text-sm text-muted-foreground">送信元: <span className="text-foreground">{state.mail.from || "未設定"}</span>(自社情報)</p>
          </div>
          <Link href="/sent-emails"><Button variant="outline" size="sm">メール(送信)の画面を開く</Button></Link>
        </div>
        <div className="flex flex-col gap-3 rounded-lg border border-border p-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <p className="font-medium">受信(IMAP)</p>
            <p className="text-sm text-muted-foreground">最後の取込: <span className="text-foreground">{state.mail.last_import_at ?? "まだありません"}</span>(15分ごと)</p>
          </div>
          <Link href="/received-emails"><Button variant="outline" size="sm">メール(受信)の画面を開く</Button></Link>
        </div>
      </section>

      {/* サーバの仕事 */}
      <section className="bg-card border border-border rounded-lg p-5 space-y-2">
        <div className="flex items-center gap-2">
          <Server className="w-4 h-4 text-muted-foreground" />
          <h2 className="text-sm text-muted-foreground">サーバの仕事</h2>
          <Pill kind="off">見るだけ</Pill>
        </div>
        <p className="text-xs text-muted-foreground">サーバで動く仕事です。この画面からは変えられません(本番デプロイの前に点検します)。</p>
        <div className="rounded-lg border border-border p-4">
          <p className="font-medium">本番DBのバックアップ(毎日 3:00)</p>
          <p className="text-sm text-muted-foreground">保存先: Google Drive ・ 状態は本番デプロイの前の点検と、毎朝の点検で確かめます</p>
        </div>
      </section>

      {/* 変更の記録 */}
      <section className="bg-card border border-border rounded-lg p-5">
        <div className="flex items-center gap-2 mb-2">
          <History className="w-4 h-4 text-muted-foreground" />
          <h2 className="text-sm text-muted-foreground">変更の記録</h2>
        </div>
        <p className="text-xs text-muted-foreground mb-3">誰が・いつ・何を変えたか。秘密の値(鍵・URL)は記録しません。</p>
        {state.changes.length === 0 ? (
          <p className="text-sm text-muted-foreground">まだ変更はありません</p>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="text-muted-foreground text-left border-b border-border">
                <th className="py-2 pr-3 font-normal">日時</th>
                <th className="py-2 pr-3 font-normal">変えた人</th>
                <th className="py-2 font-normal">内容</th>
              </tr>
            </thead>
            <tbody>
              {state.changes.map((c, i) => (
                <tr key={i} className="border-b border-border/60">
                  <td className="py-2 pr-3 whitespace-nowrap">{c.at}</td>
                  <td className="py-2 pr-3 whitespace-nowrap">{c.by ?? "—"}</td>
                  <td className="py-2">{c.summary}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {/* 開くたびに作り直す(key)ことで、入力欄を毎回その送り先の値から始める */}
      {dialog?.kind === "notify" && (
        <NotifyTargetDialog
          key={`notify-${dialog.target?.source ?? "new"}-${dialog.target?.id ?? "x"}`}
          open
          target={dialog.target}
          cipherReady={state.cipher_ready}
          loading={busy}
          onClose={() => setDialog(null)}
          onSave={save}
        />
      )}
      {dialog?.kind === "store" && (
        <StoreTargetDialog
          key={`store-${dialog.target?.source ?? "new"}-${dialog.target?.id ?? "x"}`}
          open
          target={dialog.target}
          cipherReady={state.cipher_ready}
          loading={busy}
          onClose={() => setDialog(null)}
          onSave={save}
        />
      )}
    </div>
  );
}
