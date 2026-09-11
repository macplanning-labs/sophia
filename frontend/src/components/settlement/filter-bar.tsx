"use client";

import type { MonthOption, ClientOption, PartnerOption, ProjectOption } from "@/lib/types";

interface Props {
  month: string;
  clientId: string;
  partnerId: string;
  projectId: string;
  status: string;
  onMonthChange: (v: string) => void;
  onClientChange: (v: string) => void;
  onPartnerChange: (v: string) => void;
  onProjectChange: (v: string) => void;
  onStatusChange: (v: string) => void;
  onReset: () => void;
  availableMonths: MonthOption[];
  clients: ClientOption[];
  partners: PartnerOption[];
  projects: ProjectOption[];
}

const selectClass = "bg-background border border-border text-foreground rounded-md px-2.5 py-1.5 text-[0.8125rem] font-sans appearance-auto focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary/15";
const labelClass = "text-[0.6875rem] text-muted-foreground font-medium";

export function FilterBar({
  month, clientId, partnerId, projectId, status,
  onMonthChange, onClientChange, onPartnerChange, onProjectChange, onStatusChange, onReset,
  availableMonths, clients, partners, projects,
}: Props) {
  return (
    <div className="bg-card border border-border rounded-[0.625rem] px-4 py-3 flex items-end gap-3 flex-wrap mb-4 shadow-sm">
      <div className="flex flex-col gap-1">
        <label className={labelClass}>対象年月</label>
        <select className={selectClass} value={month} onChange={(e) => onMonthChange(e.target.value)}>
          <option value="">すべて</option>
          {availableMonths.map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className={labelClass}>クライアント</label>
        <select className={selectClass} value={clientId} onChange={(e) => onClientChange(e.target.value)}>
          <option value="">すべて</option>
          {clients.map((c) => <option key={c.id} value={String(c.id)}>{c.name}</option>)}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className={labelClass}>パートナー企業</label>
        <select className={selectClass} value={partnerId} onChange={(e) => onPartnerChange(e.target.value)}>
          <option value="">すべて</option>
          {partners.map((p) => <option key={p.partner_id} value={p.partner_id}>{p.name}</option>)}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className={labelClass}>プロジェクト</label>
        <select className={selectClass} value={projectId} onChange={(e) => onProjectChange(e.target.value)}>
          <option value="">すべて</option>
          {projects.map((p) => <option key={p.project_id} value={p.project_id}>{p.name}</option>)}
        </select>
      </div>

      <div className="flex flex-col gap-1">
        <label className={labelClass}>発行ステータス</label>
        <select className={selectClass} value={status} onChange={(e) => onStatusChange(e.target.value)}>
          <option value="">すべて</option>
          <option value="pending">未確定</option>
          <option value="invoice_issued">請求書発行済</option>
          <option value="notice_issued">支払通知書発行済</option>
          <option value="complete">すべて完了</option>
        </select>
      </div>

      <button
        onClick={onReset}
        className="text-xs text-muted-foreground hover:text-foreground transition-colors px-2 py-1.5 ml-auto"
      >
        リセット
      </button>
    </div>
  );
}
