"use client";

import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchCompanyInfo, updateCompanyInfo } from "@/lib/api";
import { FormField, FormInput } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { ACCOUNT_NUMBER_INPUT_PROPS, accountNumberFormError } from "@/lib/master-form";

const emptyForm = {
  name: "", postal_code: "", address: "", tel: "", fax: "",
  representative_title: "", representative_name: "", registration_no: "",
  responsible_person: "", contact_person: "",
  bank_name: "", bank_branch: "", account_type: "", account_number: "", account_name: "",
  notice_approval_threshold: "", token_expiry_days: "",
  target_margin_direct: "", target_margin_subcontract: "",
};

/** マスタメンテ画面内の「自社情報」タブ用パネル。s_company_infoは単一行+機密情報(口座)を
 * 含むため、他マスタのような汎用一覧CRUDエンジンには乗せず専用フォームで表示・更新する。
 * メールの設定(送信アカウント・受信の除外ドメイン)は「メール」画面の ⚙ に移した(DEMO-000196)。
 * ここでは送らないので、保存してもメールの設定は変わらない。 */
export function CompanyInfoPanel() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState(emptyForm);

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
      notice_approval_threshold: info.notice_approval_threshold != null ? String(info.notice_approval_threshold) : "",
      token_expiry_days: info.token_expiry_days != null ? String(info.token_expiry_days) : "",
      target_margin_direct: String(info.target_margin_direct),
      target_margin_subcontract: String(info.target_margin_subcontract),
    });
  }, [data]);

  const saveMutation = useMutation({
    mutationFn: () => updateCompanyInfo({
      ...form,
      notice_approval_threshold: form.notice_approval_threshold ? Number(form.notice_approval_threshold) : null,
      token_expiry_days: form.token_expiry_days ? Number(form.token_expiry_days) : null,
      target_margin_direct: Number(form.target_margin_direct) || 0,
      target_margin_subcontract: Number(form.target_margin_subcontract) || 0,
    }),
    onSuccess: (res) => {
      if (!res.success) { toast.error(res.error || "保存に失敗しました"); return; }
      queryClient.invalidateQueries({ queryKey: ["company-info"] });
      toast.success("保存しました");
    },
    onError: (e: Error) => toast.error(`保存に失敗しました: ${e.message}`),
  });

  const set = (key: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => {
    setForm((p) => ({ ...p, [key]: e.target.value }));
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
        </div>
      </section>

      <section className="bg-card border border-border rounded-lg p-4">
        <h3 className="text-sm font-medium text-foreground mb-3">振込先口座</h3>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="銀行名"><FormInput value={form.bank_name} onChange={set("bank_name")} /></FormField>
          <FormField label="支店名"><FormInput value={form.bank_branch} onChange={set("bank_branch")} /></FormField>
          <FormField label="口座種別"><FormInput value={form.account_type} onChange={set("account_type")} /></FormField>
          <FormField label="口座番号"><FormInput {...ACCOUNT_NUMBER_INPUT_PROPS} value={form.account_number} onChange={set("account_number")} /></FormField>
          <FormField label="口座名義" className="col-span-2"><FormInput value={form.account_name} onChange={set("account_name")} /></FormField>
        </div>
      </section>

      <section className="bg-card border border-border rounded-lg p-4">
        <h3 className="text-sm font-medium text-foreground mb-3">利益の目安</h3>
        <div className="grid grid-cols-2 gap-4">
          <FormField label="利益の目安(直受け %)"><FormInput type="number" min="0" max="100" value={form.target_margin_direct} onChange={set("target_margin_direct")} placeholder="20" /></FormField>
          <FormField label="利益の目安(下請け %)"><FormInput type="number" min="0" max="100" value={form.target_margin_subcontract} onChange={set("target_margin_subcontract")} placeholder="8" /></FormField>
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
          onClick={() => {
            // 口座番号の誤りは、送る前に理由を出す(サーバも同じ規則で 400 を返す)
            const accountError = accountNumberFormError(form.account_number);
            if (accountError) {
              toast.error(accountError);
              return;
            }
            saveMutation.mutate();
          }}
        >
          {saveMutation.isPending ? "保存中..." : "保存"}
        </Button>
      </div>
    </div>
  );
}
