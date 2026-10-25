"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { FormField, FormModal, FormTextarea } from "@/components/ui/form-modal";
import {
  fetchMailReceiveSettings,
  testImapConnection,
  triggerMailFetch,
  unlockImap,
  updateMailReceiveSettings,
  type MailReceiveSettings,
} from "@/lib/api";
import { isApiOk } from "@/lib/api-result";
import { formatSentAt } from "@/lib/sent-emails";

const QUERY_KEY = ["mail-receive-settings"];

/**
 * メール > 受信 の ⚙ で開く「受信の設定」。DEMO-000196
 * 受信まわりを1か所で確かめられるようにする:
 * - 受信のアカウント(見るだけ。送信と共通なので、変えるのは メール > 送信 の ⚙)
 * - 受信の状態(最後に取り込んだ日時・ロック)と、ロック解除・今すぐメールチェック・受信の接続確認
 * - 除外するメールドメイン(編集して保存)
 */
export function ReceiveSettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { data, isLoading } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: fetchMailReceiveSettings,
    enabled: open,
  });
  if (!open) return null;
  if (isLoading || !data) {
    return (
      <FormModal open title="受信の設定" size="lg" loading onSubmit={() => {}} onClose={onClose}>
        <div className="h-40 bg-muted rounded-lg animate-pulse" />
      </FormModal>
    );
  }
  return <ReceiveSettingsForm settings={data} onClose={onClose} />;
}

function ReceiveSettingsForm({ settings, onClose }: { settings: MailReceiveSettings; onClose: () => void }) {
  const qc = useQueryClient();
  const [domains, setDomains] = useState(settings.excluded_mail_domains ?? "");
  const refresh = () => {
    qc.invalidateQueries({ queryKey: QUERY_KEY });
    qc.invalidateQueries({ queryKey: ["imap-lock-status"] });
  };

  const save = useMutation({
    mutationFn: () => updateMailReceiveSettings(domains),
    onSuccess: (res) => {
      if (!isApiOk(res)) { toast.error(res.error || "保存に失敗しました"); return; }
      refresh();
      // 「取引先ドメインのみ」の表示とホームの件数(どちらも received-emails)が変わる
      qc.invalidateQueries({ queryKey: ["received-emails"] });
      toast.success("保存しました");
      onClose();
    },
    onError: (e: Error) => toast.error(`保存に失敗しました: ${e.message}`),
  });

  const testReceive = useMutation({
    mutationFn: testImapConnection,
    onSuccess: (res) => {
      if (isApiOk(res)) toast.success("受信の接続確認: 成功しました");
      else toast.error(`受信の接続確認: 失敗（${res.error || "不明なエラー"}）`);
    },
    onError: (e: Error) => toast.error(`受信の接続確認: 失敗（${e.message}）`),
  });

  const unlock = useMutation({
    mutationFn: unlockImap,
    onSuccess: (res) => {
      if (!isApiOk(res)) { toast.error(`ロックの解除に失敗しました（${res.error || "不明なエラー"}）`); return; }
      refresh();
      toast.success("ロックを解除しました");
    },
    onError: (e: Error) => toast.error(`ロックの解除に失敗しました（${e.message}）`),
  });

  const fetchNow = useMutation({
    mutationFn: triggerMailFetch,
    onSuccess: (res) => {
      refresh();
      qc.invalidateQueries({ queryKey: ["received-emails"] });
      // 取り込みは途中で一部失敗しても 200 で返り、ok=false と結果の要約(error)が入る
      if (isApiOk(res)) toast.success("メールチェック完了");
      else toast.error(`メールチェック: 一部失敗しました（${res.error || "不明なエラー"}）`);
    },
    onError: (e: Error) => toast.error(`メールチェックに失敗しました（${e.message}）`),
  });

  const { imap } = settings;

  return (
    <FormModal open title="受信の設定" size="lg" loading={save.isPending} submitLabel="保存" onSubmit={() => save.mutate()} onClose={onClose}>
      <div className="space-y-6">
        <section className="space-y-2">
          <h3 className="font-semibold text-sm border-b border-border pb-2">受信のアカウント</h3>
          <dl className="grid grid-cols-[10rem_1fr] gap-y-1 text-sm">
            <dt className="text-muted-foreground">受信サーバ</dt>
            <dd className="text-foreground">{imap.host}:{imap.port}</dd>
            <dt className="text-muted-foreground">アカウント</dt>
            <dd className="text-foreground">{imap.configured ? imap.user : <span className="text-red-400">未設定</span>}</dd>
          </dl>
          <p className="text-[11px] text-muted-foreground">
            受信は、送信と同じアカウント(ユーザー・パスワード)を使います。変えるときは メール &gt; 送信 の ⚙「メールアカウント」で設定してください。
          </p>
        </section>

        <section className="space-y-2">
          <h3 className="font-semibold text-sm border-b border-border pb-2">受信の状態</h3>
          <dl className="grid grid-cols-[10rem_1fr] gap-y-1 text-sm">
            <dt className="text-muted-foreground">最後に取り込んだ日時</dt>
            <dd className="text-foreground">{settings.last_processed_at ? formatSentAt(settings.last_processed_at) : "—"}</dd>
            <dt className="text-muted-foreground">ロック</dt>
            <dd className="text-foreground">
              {settings.locked ? (
                <span className="text-red-400">ロック中（{settings.lock_reason || "理由不明"}）</span>
              ) : (
                "なし"
              )}
            </dd>
          </dl>
          {settings.locked && (
            <p className="text-[11px] text-muted-foreground">
              ログインの失敗が続いたため、取り込みを止めています。パスワードを直したあと「ロックを解除」を押してください。
            </p>
          )}
          <div className="flex flex-wrap gap-2 pt-1">
            {/* ロックの解除は消さず、ロックされていないときは非活性にする(他のボタンの位置がずれない) */}
            <Button
              variant="outline"
              size="sm"
              className="border-border"
              disabled={!settings.locked || unlock.isPending}
              title={!settings.locked ? "ロックされていません" : undefined}
              onClick={() => unlock.mutate()}
            >
              {unlock.isPending ? "解除中..." : "ロックを解除"}
            </Button>
            <Button variant="outline" size="sm" className="border-border" disabled={fetchNow.isPending} onClick={() => fetchNow.mutate()}>
              {fetchNow.isPending ? "チェック中..." : "今すぐメールチェック"}
            </Button>
            <Button variant="outline" size="sm" className="border-border" disabled={testReceive.isPending} onClick={() => testReceive.mutate()}>
              {testReceive.isPending ? "確認中..." : "受信の接続確認"}
            </Button>
          </div>
        </section>

        <section className="space-y-2">
          <h3 className="font-semibold text-sm border-b border-border pb-2">取り込みの設定</h3>
          <FormField label="除外するメールドメイン（1行に1つ）">
            <FormTextarea value={domains} onChange={(e) => setDomains(e.target.value)} placeholder={"例: example.co.jp"} rows={4} />
            <p className="text-[11px] text-muted-foreground mt-1">
              自社など、取引先として扱わないドメインを登録します。受信メールの「取引先ドメインのみ」表示と、ホームの「受信メール」の件数から外れます（メール自体は取り込まれ、「全件」で見られます）。
            </p>
          </FormField>
        </section>
      </div>
    </FormModal>
  );
}
