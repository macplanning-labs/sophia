"use client";

import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchCompanyInfo, updateCompanyInfo, testCompanyEmail } from "@/lib/api";
import { FormField, FormInput } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";

const emptyForm = {
  name: "", postal_code: "", address: "", tel: "", fax: "",
  representative_title: "", representative_name: "", registration_no: "",
  responsible_person: "", contact_person: "",
  bank_name: "", bank_branch: "", account_type: "", account_number: "", account_name: "",
  stamp_image: "", logo_image: "",
  email_host: "", email_port: "", email_use_tls: true, email_host_user: "", email_host_password: "",
  default_from_email: "", notice_approval_threshold: "", token_expiry_days: "",
};

/** マスタメンテ画面内の「自社情報」タブ用パネル。s_company_infoは単一行+機密情報(SMTPパスワード)を
 * 含むため、他マスタのような汎用一覧CRUDエンジンには乗せず専用フォームで表示・更新する。 */
export function CompanyInfoPanel() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState(emptyForm);
  const [showPassword, setShowPassword] = useState(false);
  const [changePassword, setChangePassword] = useState(false);

  const { data, isLoading } = useQuery({
    queryKey: ["company-info"],
    queryFn: fetchCompanyInfo,
  });

  useEffect(() => {
    const info = data?.company_info;
    if (!info) return;
    setForm({
      name: info.name, postal_code: info.postal_code, address: info.address, tel: info.tel, fax: info.fax,
      representative_title: info.representative_title, representative_name: info.representative_name,
      registration_no: info.registration_no, responsible_person: info.responsible_person, contact_person: info.contact_person,
      bank_name: info.bank_name, bank_branch: info.bank_branch, account_type: info.account_type,
      account_number: info.account_number, account_name: info.account_name,
      stamp_image: info.stamp_image, logo_image: info.logo_image,
      email_host: info.email_host, email_port: info.email_port != null ? String(info.email_port) : "",
      email_use_tls: info.email_use_tls, email_host_user: info.email_host_user, email_host_password: "",
      default_from_email: info.default_from_email,
      notice_approval_threshold: info.notice_approval_threshold != null ? String(info.notice_approval_threshold) : "",
      token_expiry_days: info.token_expiry_days != null ? String(info.token_expiry_days) : "",
    });
    setChangePassword(false);
  }, [data]);

  const saveMutation = useMutation({
    mutationFn: () => updateCompanyInfo({
      ...form,
      // パスワードマネージャーの自動入力等で欄に値が入っていても、
      // 「パスワードを変更する」を明示的にチェックしていない限り送信しない。
      email_host_password: changePassword ? form.email_host_password : "",
      email_port: form.email_port ? Number(form.email_port) : null,
      notice_approval_threshold: form.notice_approval_threshold ? Number(form.notice_approval_threshold) : null,
      token_expiry_days: form.token_expiry_days ? Number(form.token_expiry_days) : null,
    }),
    onSuccess: (res) => {
      if (!res.success) { toast.error(res.error || "保存に失敗しました"); return; }
      queryClient.invalidateQueries({ queryKey: ["company-info"] });
      setForm((p) => ({ ...p, email_host_password: "" }));
      setChangePassword(false);
      toast.success("保存しました");
    },
    onError: (e: Error) => toast.error(`保存に失敗しました: ${e.message}`),
  });

  const testEmailMutation = useMutation({
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

  if (isLoading) {
    return <div className="h-64 bg-muted rounded-lg animate-pulse" />;
  }

  return (
    <div className="space-y-6">
      <p className="text-sm text-muted-foreground">
        請求書・注文書PDFや自動送信メールに使われる自社情報です（全社で1件のみ）。
      </p>

      <section className="bg-card border border-border rounded-lg p-4">
        <h3 className="text-sm font-medium text-foreground mb-3">基本情報</h3>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="会社名"><FormInput value={form.name} onChange={set("name")} /></FormField>
          <FormField label="登録番号（インボイス）"><FormInput value={form.registration_no} onChange={set("registration_no")} /></FormField>
          <FormField label="郵便番号"><FormInput value={form.postal_code} onChange={set("postal_code")} /></FormField>
          <FormField label="住所"><FormInput value={form.address} onChange={set("address")} /></FormField>
          <FormField label="電話番号"><FormInput value={form.tel} onChange={set("tel")} /></FormField>
          <FormField label="FAX番号"><FormInput value={form.fax} onChange={set("fax")} /></FormField>
          <FormField label="代表者役職"><FormInput value={form.representative_title} onChange={set("representative_title")} /></FormField>
          <FormField label="代表者氏名"><FormInput value={form.representative_name} onChange={set("representative_name")} /></FormField>
          <FormField label="業務責任者"><FormInput value={form.responsible_person} onChange={set("responsible_person")} /></FormField>
          <FormField label="連絡窓口担当者"><FormInput value={form.contact_person} onChange={set("contact_person")} /></FormField>
          <FormField label="印影画像パス"><FormInput value={form.stamp_image} onChange={set("stamp_image")} /></FormField>
          <FormField label="ロゴ画像パス"><FormInput value={form.logo_image} onChange={set("logo_image")} /></FormField>
        </div>
      </section>

      <section className="bg-card border border-border rounded-lg p-4">
        <h3 className="text-sm font-medium text-foreground mb-3">振込先口座</h3>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="銀行名"><FormInput value={form.bank_name} onChange={set("bank_name")} /></FormField>
          <FormField label="支店名"><FormInput value={form.bank_branch} onChange={set("bank_branch")} /></FormField>
          <FormField label="口座種別"><FormInput value={form.account_type} onChange={set("account_type")} /></FormField>
          <FormField label="口座番号"><FormInput value={form.account_number} onChange={set("account_number")} /></FormField>
          <FormField label="口座名義" className="col-span-2"><FormInput value={form.account_name} onChange={set("account_name")} /></FormField>
        </div>
      </section>

      <section className="bg-card border border-border rounded-lg p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-medium text-foreground">メール送信設定（SMTP）</h3>
          <button
            type="button"
            onClick={() => testEmailMutation.mutate()}
            disabled={testEmailMutation.isPending}
            className="px-2 py-1 bg-muted hover:bg-muted disabled:opacity-40 text-foreground text-xs rounded transition-colors"
          >
            {testEmailMutation.isPending ? "送信中..." : "テスト送信"}
          </button>
        </div>
        <p className="text-xs text-muted-foreground -mt-2 mb-3">
          保存済みの設定で、ログイン中の自分自身にテストメールを送信します（未保存の変更は反映されません）。
        </p>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="SMTPホスト"><FormInput value={form.email_host} onChange={set("email_host")} placeholder="smtp.gmail.com" /></FormField>
          <FormField label="SMTPポート"><FormInput type="number" value={form.email_port} onChange={set("email_port")} placeholder="587" /></FormField>
          <FormField label="SMTPユーザー"><FormInput value={form.email_host_user} onChange={set("email_host_user")} /></FormField>
          <FormField label={`SMTPパスワード${data?.company_info?.has_smtp_password ? "（設定済み）" : "（未設定）"}`}>
            <div className="space-y-2">
              <label className="flex items-center gap-2 text-sm text-foreground cursor-pointer">
                <input
                  type="checkbox"
                  checked={changePassword}
                  onChange={(e) => {
                    const checked = e.target.checked;
                    setChangePassword(checked);
                    if (!checked) setForm((p) => ({ ...p, email_host_password: "" }));
                  }}
                />
                パスワードを変更する
              </label>
              {changePassword && (
                <>
                  <FormInput
                    type={showPassword ? "text" : "password"}
                    value={form.email_host_password}
                    onChange={set("email_host_password")}
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
      </section>

      <section className="bg-card border border-border rounded-lg p-4">
        <h3 className="text-sm font-medium text-foreground mb-3">承認・トークン設定</h3>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="支払通知書の上司承認基準金額（円）">
            <FormInput type="number" value={form.notice_approval_threshold} onChange={set("notice_approval_threshold")} placeholder="500000" />
          </FormField>
          <FormField label="パートナートークンURLの有効期限（日）">
            <FormInput type="number" value={form.token_expiry_days} onChange={set("token_expiry_days")} placeholder="14" />
          </FormField>
        </div>
      </section>

      <div className="flex justify-end">
        <Button
          className="bg-primary hover:bg-primary/90 text-primary-foreground"
          disabled={saveMutation.isPending}
          onClick={() => saveMutation.mutate()}
        >
          {saveMutation.isPending ? "保存中..." : "保存"}
        </Button>
      </div>
    </div>
  );
}
