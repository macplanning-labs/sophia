"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { FormField, FormInput, FormModal } from "@/components/ui/form-modal";
import {
  fetchMailAccount,
  testCompanyEmail,
  updateMailAccount,
  type MailAccount,
} from "@/lib/api";

/**
 * メール > 送信 の ⚙ で開く「メールアカウント」。DEMO-000196
 * 送信(SMTP)の設定だが、受信の取り込み(IMAP)も同じユーザー・パスワードを使うので「送信・受信で共通」と明記する。
 * 受信の接続確認・ロック解除・今すぐメールチェックは、受信の ⚙(ReceiveSettingsDialog)に置く。
 */
export function MailAccountDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { data, isLoading } = useQuery({
    queryKey: ["mail-account"],
    queryFn: fetchMailAccount,
    enabled: open,
  });
  if (!open) return null;
  if (isLoading || !data) {
    return (
      <FormModal open title="メールアカウント" size="lg" loading onSubmit={() => {}} onClose={onClose}>
        <div className="h-40 bg-muted rounded-lg animate-pulse" />
      </FormModal>
    );
  }
  return <MailAccountForm initial={data.account} onClose={onClose} />;
}

function MailAccountForm({ initial, onClose }: { initial: MailAccount; onClose: () => void }) {
  const qc = useQueryClient();
  const [form, setForm] = useState({
    email_host: initial.email_host ?? "",
    email_port: initial.email_port != null ? String(initial.email_port) : "",
    email_use_tls: initial.email_use_tls,
    email_host_user: initial.email_host_user ?? "",
    default_from_email: initial.default_from_email ?? "",
  });
  const [changePassword, setChangePassword] = useState(false);
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  const save = useMutation({
    mutationFn: () => updateMailAccount({
      ...form,
      email_port: form.email_port ? Number(form.email_port) : null,
      // パスワードマネージャーの自動入力等で欄に値が入っていても、「パスワードを変更する」をチェックしていない限り送らない
      email_host_password: changePassword ? password : "",
    }),
    onSuccess: (res) => {
      if (!res.success) { toast.error(res.error || "保存に失敗しました"); return; }
      qc.invalidateQueries({ queryKey: ["mail-account"] });
      qc.invalidateQueries({ queryKey: ["imap-lock-status"] });
      toast.success("保存しました");
      onClose();
    },
    onError: (e: Error) => toast.error(`保存に失敗しました: ${e.message}`),
  });

  const testSend = useMutation({
    mutationFn: testCompanyEmail,
    onSuccess: (res) => {
      if (res.success) toast.success(`テストメールを送信しました（送信先: ${res.to}）`);
      else toast.error(`テストメール送信失敗: ${res.error || "不明なエラー"}`);
    },
    onError: (e: Error) => toast.error(`テストメール送信失敗: ${e.message}`),
  });

  const set = (key: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = key === "email_use_tls" ? e.target.checked : e.target.value;
    setForm((p) => ({ ...p, [key]: value }));
  };

  return (
    <FormModal
      open
      title="メールアカウント"
      size="lg"
      loading={save.isPending}
      submitLabel="保存"
      onSubmit={() => save.mutate()}
      onClose={onClose}
      footerStart={
        <Button variant="outline" size="sm" className="border-border" disabled={testSend.isPending} onClick={() => testSend.mutate()}>
          {testSend.isPending ? "送信中..." : "テスト送信"}
        </Button>
      }
    >
      <p className="text-xs text-muted-foreground mb-3">
        Sophia がメールを送るときのアカウントです。受信メールの取り込みも、同じユーザーとパスワードを使います（送信・受信で共通）。
        テスト送信は保存済みの設定で行い、ログイン中の自分自身に届きます（未保存の変更は反映されません）。受信の接続確認は、メール &gt; 受信 の ⚙ にあります。
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <FormField label="SMTPホスト"><FormInput value={form.email_host} onChange={set("email_host")} placeholder="smtp.gmail.com" /></FormField>
        <FormField label="SMTPポート"><FormInput type="number" value={form.email_port} onChange={set("email_port")} placeholder="587" /></FormField>
        <FormField label="SMTPユーザー"><FormInput value={form.email_host_user} onChange={set("email_host_user")} /></FormField>
        <FormField label={`SMTPパスワード${initial.has_smtp_password ? "（設定済み）" : "（未設定）"}`}>
          <div className="space-y-2">
            <label className="flex items-center gap-2 text-sm text-foreground cursor-pointer">
              <input
                type="checkbox"
                checked={changePassword}
                onChange={(e) => {
                  setChangePassword(e.target.checked);
                  if (!e.target.checked) setPassword("");
                }}
              />
              パスワードを変更する
            </label>
            {changePassword && (
              <>
                <FormInput
                  type={showPassword ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="新しいパスワードを入力"
                  autoComplete="off"
                  data-1p-ignore
                  data-lpignore="true"
                />
                <label className="flex items-center gap-2 text-sm text-foreground cursor-pointer">
                  <input type="checkbox" checked={showPassword} onChange={(e) => setShowPassword(e.target.checked)} />
                  パスワードを表示する
                </label>
              </>
            )}
          </div>
        </FormField>
        <FormField label="送信元メールアドレス"><FormInput type="email" value={form.default_from_email} onChange={set("default_from_email")} /></FormField>
        <FormField label="TLSを使用する">
          <label className="flex items-center gap-2 h-[38px] text-sm text-foreground">
            <input type="checkbox" checked={form.email_use_tls} onChange={set("email_use_tls")} />
            有効
          </label>
        </FormField>
      </div>
    </FormModal>
  );
}
