"use client";

// 送った書類の「訂正版」と「取り消し」の画面部品(DEMO-000224)。請求書・支払通知書の詳細で共通に使う。
// 方針(docs/spec/要件定義書.md §3.11): 送った書類は消さず・書き換えない。直すときは訂正版(前の版は差し替え済みで残る)、
// なかったことにするときは取り消し(取り消し済みで残る)。どちらも理由を残す。
import Link from "next/link";
import { useState } from "react";
import { FormField, FormModal, FormTextarea } from "@/components/ui/form-modal";

export const REASON_MAX = 500;

/** 版と状態の表示用の情報(詳細のAPIの値から作る) */
export interface RevisionView {
  revision: number;
  supersedesId: string | null;
  revisionReason: string | null;
  cancelledAt: string | null;
  cancelReason: string | null;
  supersededBy: { id: string; sent: boolean; reason: string | null } | null;
}

/** 送った(承諾した)書類か */
export function isLocked(sentAt: unknown, acceptedAt: unknown): boolean {
  return !!sentAt || !!acceptedAt;
}

/** 生きている版か(取り消し済みでも、差し替え済みでもない) */
export function isCurrentRevision(v: RevisionView): boolean {
  return !v.cancelledAt && !v.supersededBy;
}

/** 状態の名前(通常は null) */
export function revisionLabel(v: RevisionView): "取り消し済み" | "差し替え済み" | null {
  if (v.cancelledAt) return "取り消し済み";
  if (v.supersededBy) return "差し替え済み";
  return null;
}

/** 版と状態のバッジ(詳細の見出しの横。一覧の通知番号の横にも使う)。版1で通常の状態なら、何も出さない */
export function RevisionBadges({ revision, label }: { revision: number; label: "取り消し済み" | "差し替え済み" | null }) {
  if (revision <= 1 && !label) return null;
  return (
    <>
      {revision > 1 && (
        <span className="ml-2 inline-block rounded-md border border-border px-2 py-0.5 text-xs text-muted-foreground align-middle">
          版 {revision}
        </span>
      )}
      {label && (
        <span
          className={
            "ml-2 inline-block rounded-md border px-2 py-0.5 text-xs align-middle " +
            (label === "取り消し済み"
              ? "border-red-600/50 text-red-400"
              : "border-amber-600/50 text-amber-400")
          }
        >
          {label}
        </span>
      )}
    </>
  );
}

/** 版・取り消しの案内(詳細の上部)。前の版・新しい版へのリンク付き */
export function RevisionBanners({
  view,
  hrefOf,
}: {
  view: RevisionView;
  /** 別の版の詳細へのリンク */
  hrefOf: (id: string) => string;
}) {
  // 取り消しも差し替えも版の履歴も無い通常の書類では、何も出さない(空の枠が、余白を取らないように)
  if (!view.cancelledAt && !view.supersededBy && !view.supersedesId) return null;
  return (
    <div className="space-y-2 mb-4">
      {view.cancelledAt && (
        <div className="rounded-lg border border-red-600/40 bg-red-950/30 px-4 py-3 text-sm text-red-300">
          この書類は取り消されました({String(view.cancelledAt).slice(0, 10)})。
          {view.cancelReason && <>理由: {view.cancelReason}。</>}
          書類は消えずに残っています。
        </div>
      )}
      {view.supersededBy && (
        <div className="rounded-lg border border-amber-600/40 bg-amber-950/30 px-4 py-3 text-sm text-amber-300">
          版 {view.revision + 1} に差し替えられました。
          {view.supersededBy.reason && <>理由: {view.supersededBy.reason}。</>}{" "}
          <Link href={hrefOf(view.supersededBy.id)} className="underline">
            新しい版を開く
          </Link>
        </div>
      )}
      {view.supersedesId && (
        <div className="rounded-lg border border-amber-600/40 bg-amber-950/30 px-4 py-3 text-sm text-amber-300">
          版 {view.revision - 1} は差し替え済みです。
          {view.revisionReason && <>変えた理由: {view.revisionReason}。</>}{" "}
          <Link href={hrefOf(view.supersedesId)} className="underline">
            版 {view.revision - 1} を見る
          </Link>
        </div>
      )}
    </div>
  );
}

interface ReasonModalProps {
  open: boolean;
  kind: "revise" | "cancel";
  docLabel: string;
  docNo: string;
  loading: boolean;
  error?: string | null;
  onSubmit: (reason: string) => void;
  onClose: () => void;
}

/** 理由を入力して、訂正版を作る/取り消すモーダル。開くたびに作り直す(入力が前回の内容を引き継がない) */
export function ReasonModal(props: ReasonModalProps) {
  if (!props.open) return null;
  return <ReasonModalBody {...props} />;
}

function ReasonModalBody({ kind, docLabel, docNo, loading, error, onSubmit, onClose }: ReasonModalProps) {
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);

  const isRevise = kind === "revise";
  const trimmed = reason.trim();
  const invalid = trimmed.length === 0 ? "理由を入力してください" : trimmed.length > REASON_MAX ? `理由は${REASON_MAX}文字以内で入力してください` : null;

  return (
    <FormModal
      open
      title={isRevise ? `${docLabel} ${docNo} の訂正版を作る` : `${docLabel} ${docNo} を取り消す`}
      size="md"
      loading={loading}
      submitLabel={isRevise ? "訂正版を作る" : "取り消す"}
      onSubmit={() => {
        setTouched(true);
        if (invalid) return;
        onSubmit(trimmed);
      }}
      onClose={onClose}
    >
      <p className="text-sm text-muted-foreground mb-3">
        {isRevise
          ? "同じ内容の新しい版(下書き)を作ります。いまの版は消さず、「差し替え済み」として残ります。新しい版は、承認 → 送信 → 相手の承諾を、もう一度、通します。"
          : "書類は消さず、「取り消し済み」として残ります。相手のリンクにも、取り消されたことが表示されます。取り消したあとは、元に戻せません。"}
      </p>
      <FormField label={isRevise ? "変える理由(必須)" : "取り消す理由(必須)"} required error={touched ? invalid ?? undefined : undefined}>
        <FormTextarea
          rows={3}
          value={reason}
          maxLength={REASON_MAX + 50}
          placeholder={isRevise ? "例: パートナーから、口頭で金額の訂正の依頼があった" : "例: 契約が終了したため"}
          onChange={(e) => setReason(e.target.value)}
        />
      </FormField>
      {error && <p className="text-sm text-red-400">エラー: {error}</p>}
    </FormModal>
  );
}
