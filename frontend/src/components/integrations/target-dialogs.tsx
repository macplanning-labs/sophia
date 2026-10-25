"use client";

// 外部連携の設定画面の「変更」「送り先を足す」ダイアログ(DEMO-000215。承認済みモック 2026-10-12)
// 秘密の値(Webhook の URL・サービスアカウントの鍵)は、保存後は表示しない。入れ直すときだけ入力する。

import { useState } from "react";
import { FormField, FormInput, FormModal, FormSelect } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import type { IntegrationTarget, IntegrationTargetInput } from "@/lib/api";

const NOTIFY_PROVIDERS = [
  { value: "gchat", label: "Google Chat" },
  { value: "slack", label: "Slack(準備中)", disabled: true },
  { value: "teams", label: "Microsoft Teams(準備中)", disabled: true },
  { value: "mail", label: "メール(準備中)", disabled: true },
];

const STORE_PROVIDERS = [
  { value: "gdrive", label: "Google Drive" },
  { value: "onedrive", label: "OneDrive・SharePoint(準備中)", disabled: true },
  { value: "box", label: "Box(準備中)", disabled: true },
  { value: "s3", label: "S3 互換(準備中)", disabled: true },
  { value: "nas", label: "NAS(準備中)", disabled: true },
];

function Callout({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-md border border-cyan-500/30 bg-cyan-500/5 p-3 text-xs text-cyan-200">{children}</div>
  );
}

function CipherWarning() {
  return (
    <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-300">
      暗号化の鍵がサーバに設定されていないため、秘密の設定(URL・鍵)を保存できません。システムの担当者に連絡してください。
    </div>
  );
}

/** 「設定済み(表示しません)」と「入れ直す」の切り替え */
function SecretField({
  label,
  placeholder,
  hasSecret,
  value,
  onChange,
  fileMode,
}: {
  label: string;
  placeholder: string;
  hasSecret: boolean;
  value: string | null;
  onChange: (v: string | null) => void;
  fileMode?: boolean;
}) {
  const editing = value !== null || !hasSecret;
  return (
    <FormField label={label} required={!hasSecret}>
      {editing ? (
        fileMode ? (
          <input
            type="file"
            accept="application/json,.json"
            className="w-full text-sm text-muted-foreground file:mr-3 file:rounded-md file:border file:border-border file:bg-muted file:px-3 file:py-1.5 file:text-sm file:text-foreground"
            onChange={async (e) => {
              const f = e.target.files?.[0];
              onChange(f ? await f.text() : null);
            }}
          />
        ) : (
          // type="password" にすると、ブラウザがログインの画面と思い込み、保存したパスワードを自動で入れてしまう(staging で確認)。
          // ふつうの文字の欄にして、表示だけ ● で隠す。自動入力・パスワード管理の提案も止める
          <FormInput
            type="text"
            autoComplete="off"
            spellCheck={false}
            data-1p-ignore
            data-lpignore="true"
            className="[-webkit-text-security:disc]"
            placeholder={placeholder}
            value={value ?? ""}
            onChange={(e) => onChange(e.target.value)}
          />
        )
      ) : (
        <div className="flex gap-2 items-center">
          <FormInput value="設定済み(表示しません)" disabled />
          <Button type="button" variant="outline" size="sm" onClick={() => onChange("")}>
            入れ直す
          </Button>
        </div>
      )}
    </FormField>
  );
}

interface DialogProps {
  open: boolean;
  /** 変える送り先(null なら足す。サーバの設定ファイルの送り先は id が null で、画面の設定に移す) */
  target: IntegrationTarget | null;
  cipherReady: boolean;
  loading: boolean;
  onClose: () => void;
  onSave: (id: number | null, input: IntegrationTargetInput) => void;
}

export function NotifyTargetDialog({ open, target, cipherReady, loading, onClose, onSave }: DialogProps) {
  const isDb = target?.source === "db" && target.id !== null;
  // ダイアログは開くたびに作り直す(呼び出し側で key を変える)ので、初期値はここで決める
  const [label, setLabel] = useState(target ? (target.source === "env" ? "運用監視" : target.label) : "");
  const [destName, setDestName] = useState(target?.destination_name ?? "");
  const [url, setUrl] = useState<string | null>(null);

  return (
    <FormModal
      open={open}
      title={isDb ? "送り先を変える(異常の通知)" : target ? "画面の設定に移す(異常の通知)" : "送り先を足す(異常の通知)"}
      size="md"
      loading={loading}
      submitLabel="保存"
      onClose={onClose}
      onSubmit={() =>
        onSave(isDb ? target!.id : null, {
          use_id: "ops_alert",
          label,
          provider: "gchat",
          config: { destination_name: destName.trim() },
          secret: url !== null && url.trim() !== "" ? { webhook_url: url.trim() } : null,
        })
      }
    >
      <div className="space-y-4">
        {!cipherReady && <CipherWarning />}
        <FormField label="名前(この画面と通知の記録に出ます)" required>
          <FormInput autoComplete="off" value={label} onChange={(e) => setLabel(e.target.value)} placeholder="運用監視" />
        </FormField>
        <FormField label="宛先の名前(Chat のスペース名)" required>
          <FormInput autoComplete="off" value={destName} onChange={(e) => setDestName(e.target.value)} placeholder="運用監視（STG）" />
          <p className="text-xs text-muted-foreground">この画面に「どのスペースか」を出すための名前です。Chat で見えているスペース名をそのまま入れてください。</p>
        </FormField>
        <FormField label="つなぎ先">
          <FormSelect options={NOTIFY_PROVIDERS} value="gchat" disabled={isDb} onChange={() => undefined} />
        </FormField>
        <SecretField
          label="Webhook の URL"
          placeholder="https://chat.googleapis.com/v1/spaces/…/messages?key=…&token=…"
          hasSecret={isDb && !!target?.has_secret}
          value={url}
          onChange={setUrl}
        />
        <p className="text-xs text-muted-foreground">URL は鍵を含むので、保存後は表示しません。入れ直すときだけ入力します。</p>
        <Callout>
          保存すると、すぐに点検します(形だけを確かめ、相手には送りません)。届くかどうかは「テスト送信」で確かめてください。
        </Callout>
      </div>
    </FormModal>
  );
}

export function StoreTargetDialog({ open, target, cipherReady, loading, onClose, onSave }: DialogProps) {
  const isDb = target?.source === "db" && target.id !== null;
  // ダイアログは開くたびに作り直す(呼び出し側で key を変える)ので、初期値はここで決める
  const [label, setLabel] = useState(isDb && target ? target.label : "Google Drive(共有ドライブ)");
  const [folder, setFolder] = useState(
    target?.config.root_folder_id ?? target?.destination ?? "",
  );
  const [impersonate, setImpersonate] = useState(target?.config.impersonate ?? "");
  const [keyJson, setKeyJson] = useState<string | null>(null);

  return (
    <FormModal
      open={open}
      title={isDb ? "保存先を変える(発行した書類の保存)" : target ? "画面の設定に移す(発行した書類の保存)" : "保存先を設定する(発行した書類の保存)"}
      size="md"
      loading={loading}
      submitLabel="保存"
      onClose={onClose}
      onSubmit={() =>
        onSave(isDb ? target!.id : null, {
          use_id: "issued_pdf_archive",
          label,
          provider: "gdrive",
          config: { root_folder_id: folder.trim(), impersonate: impersonate.trim() },
          secret: keyJson !== null && keyJson.trim() !== "" ? { service_account_json: keyJson } : null,
        })
      }
    >
      <div className="space-y-4">
        {!cipherReady && <CipherWarning />}
        <FormField label="名前(この画面に出ます)" required>
          <FormInput autoComplete="off" value={label} onChange={(e) => setLabel(e.target.value)} />
        </FormField>
        <FormField label="つなぎ先">
          <FormSelect options={STORE_PROVIDERS} value="gdrive" disabled={isDb} onChange={() => undefined} />
        </FormField>
        <FormField label="保存先のフォルダ ID" required>
          <FormInput autoComplete="off" value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="1bHlH4…" />
          <p className="text-xs text-muted-foreground">フォルダを開いたときのアドレス欄の「…/folders/」の後ろです。</p>
        </FormField>
        <SecretField
          label="サービスアカウントの鍵(JSON)"
          placeholder=""
          hasSecret={isDb && !!target?.has_secret}
          value={keyJson}
          onChange={setKeyJson}
          fileMode
        />
        <FormField label="なりすまし先のメールアドレス(任意)">
          <FormInput autoComplete="off" value={impersonate} onChange={(e) => setImpersonate(e.target.value)} placeholder="使わない場合は空のまま" />
        </FormField>
        <Callout>
          保存すると、すぐに点検します(フォルダが見えるか・書き込めるかを、書き込まずに確かめます)。共有ドライブでは、鍵のアカウントを「コンテンツ管理者」で追加してください。
        </Callout>
      </div>
    </FormModal>
  );
}
