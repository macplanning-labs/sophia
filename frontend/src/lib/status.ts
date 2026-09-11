/**
 * 全ドメインの statusMap 定義 — Midnight テーマ準拠
 *
 * 使い方:
 *   import { getStatus } from "@/lib/status";
 *   const st = getStatus("order", row.status);
 *   <Badge className={st.className}>{st.label}</Badge>
 */

export interface StatusDef {
  label: string;
  className: string;
}

const FALLBACK: StatusDef = {
  label: "不明",
  className: "border-slate-600/30 text-slate-400",
};

// ── ドメイン別ステータス定義 ──

// 発注注文書(PurchaseOrder)。実際の値はsrc/domain/models/partner_contract.rs::PurchaseOrderStatusと一致させる
// （旧: REPORT_RECEIVED/NOTICE_CREATED/NOTICE_CONFIRMED/PAIDが未定義で、一覧画面が生の英語ステータスを
// そのまま表示してしまうバグがあったため2026-07-15に追加）
const order: Record<string, StatusDef> = {
  DRAFT:            { label: "下書き",     className: "border-slate-500/30 text-slate-400" },
  SENT:             { label: "送付済",     className: "border-sky-500/30 text-sky-400" },
  ACCEPTED:         { label: "承諾済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  REPORT_RECEIVED:  { label: "報告書受領", className: "border-amber-500/30 text-amber-400" },
  NOTICE_CREATED:   { label: "支払通知作成", className: "border-indigo-500/30 text-indigo-400" },
  NOTICE_CONFIRMED: { label: "支払通知受諾", className: "border-sky-500/30 text-sky-400" },
  PAID:             { label: "支払済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  REJECTED:         { label: "却下",       className: "border-rose-500/30 text-rose-400" },
  CANCELLED:        { label: "取消",       className: "border-slate-500/30 text-slate-400" },
};

// 実際の値は ReceivedOrderStatus（src/domain/models/client_contract.rs）と一致させる
const receivedOrder: Record<string, StatusDef> = {
  REGISTERED:      { label: "受注登録",   className: "border-slate-500/30 text-slate-400" },
  REPORT_RECEIVED: { label: "勤怠受領",   className: "border-amber-500/30 text-amber-400" },
  REPORT_SENT:     { label: "報告書送付", className: "border-sky-500/30 text-sky-400" },
  INVOICED:        { label: "請求書処理", className: "border-indigo-500/30 text-indigo-400" },
  PAID:            { label: "入金済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
};

const invoice: Record<string, StatusDef> = {
  DRAFT:             { label: "下書き",     className: "border-slate-500/30 text-slate-400" },
  CONFIRMED:         { label: "確定",       className: "border-sky-500/30 text-sky-400" },
  ISSUED:            { label: "発行済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  PAID:              { label: "入金済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  // 請求書承認・送信ワークフロー（migrations/021、2026-07-11実装）
  PENDING_APPROVAL:  { label: "承認待ち",   className: "border-amber-500/30 text-amber-400" },
  APPROVED:          { label: "承認済",     className: "border-sky-500/30 text-sky-400" },
  SENT:              { label: "送信済",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
};

const notice: Record<string, StatusDef> = {
  DRAFT:     { label: "下書き",   className: "border-slate-500/30 text-slate-400" },
  ISSUED:    { label: "発行済",   className: "border-sky-500/30 text-sky-400" },
  SENT:      { label: "送付済",   className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
};

const timesheet: Record<string, StatusDef> = {
  PENDING:   { label: "未提出",   className: "border-slate-500/30 text-slate-400" },
  UPLOADED:  { label: "提出済",   className: "border-sky-500/30 text-sky-400" },
  PARSED:    { label: "解析済",   className: "border-cyan-500/30 text-cyan-400" },
  APPROVED:  { label: "承認済",   className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  REJECTED:  { label: "差戻し",   className: "border-rose-500/30 text-rose-400" },
  SENT:      { label: "送信済",   className: "bg-sky-500/15 text-sky-400 border-sky-500/30" },
};

const payroll: Record<string, StatusDef> = {
  DRAFT:     { label: "計算済",   className: "border-slate-500/30 text-slate-400" },
  CONFIRMED: { label: "確認済",   className: "border-sky-500/30 text-sky-400" },
  PAID:      { label: "振込済",   className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
};

const expense: Record<string, StatusDef> = {
  DRAFT:     { label: "下書き",   className: "border-slate-500/30 text-slate-400" },
  PENDING:   { label: "申請中",   className: "border-amber-500/30 text-amber-400" },
  APPROVED:  { label: "承認済",   className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  REJECTED:  { label: "却下",     className: "border-rose-500/30 text-rose-400" },
};

const task: Record<string, StatusDef> = {
  PENDING:   { label: "未着手",   className: "border-slate-500/30 text-slate-400" },
  DONE:      { label: "完了",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  SKIPPED:   { label: "スキップ", className: "border-amber-500/30 text-amber-400" },
};

const settlement: Record<string, StatusDef> = {
  ISSUED:    { label: "発行済",   className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  PENDING:   { label: "未発行",   className: "border-slate-500/30 text-slate-400" },
};

const generic: Record<string, StatusDef> = {
  ACTIVE:    { label: "有効",     className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30" },
  INACTIVE:  { label: "無効",     className: "border-slate-500/30 text-slate-400" },
};

// ── ドメインマップ ──

const domainMap: Record<string, Record<string, StatusDef>> = {
  order,
  receivedOrder,
  invoice,
  notice,
  timesheet,
  payroll,
  expense,
  task,
  settlement,
  generic,
};

/**
 * ドメインとステータスキーからStatusDefを取得
 * @param domain ドメイン名（"order", "timesheet", "payroll" etc.）
 * @param status ステータスキー（"DRAFT", "CONFIRMED" etc.）
 */
export function getStatus(domain: string, status: string): StatusDef {
  return domainMap[domain]?.[status] ?? generic[status] ?? { ...FALLBACK, label: status };
}

/**
 * ドメインの全ステータス定義を取得（フィルタドロップダウン用）
 */
export function getAllStatuses(domain: string): Array<{ value: string } & StatusDef> {
  const map = domainMap[domain] ?? {};
  return Object.entries(map).map(([value, def]) => ({ value, ...def }));
}
