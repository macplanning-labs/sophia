"use client";

import type { Dispatch, SetStateAction } from "react";
import { SidePanel } from "@/components/ui/side-panel";
import { Button } from "@/components/ui/button";
import { FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { formatYen, formatPct, targetBadge, warningTone } from "@/lib/assignment-display";
import { validateForm, type AssignmentForm, type TermsFields } from "@/lib/assignment-form";
import type { AssignmentPreview } from "@/lib/types";

interface PartnerOption {
  partner_id: string;
  name: string;
}

interface Props {
  form: AssignmentForm;
  setForm: Dispatch<SetStateAction<AssignmentForm | null>>;
  engineerName: string;
  projectLabel: string;
  partners: PartnerOption[];
  preview: AssignmentPreview | undefined;
  previewLoading: boolean;
  previewError: string | null;
  creating: boolean;
  onCreate: () => void;
  onClose: () => void;
}

function TermsEditor({ value, onChange }: { value: TermsFields; onChange: (t: TermsFields) => void }) {
  const set = (k: keyof TermsFields, v: string) => onChange({ ...value, [k]: v });
  return (
    <div className="grid grid-cols-2 gap-3">
      <FormField label="単価(月額・円)" required>
        <FormInput type="number" inputMode="numeric" min={1} value={value.base_rate} onChange={(e) => set("base_rate", e.target.value)} placeholder="700000" />
      </FormField>
      <FormField label="工数(人月)">
        <FormInput type="number" step="0.1" min={0} value={value.effort} onChange={(e) => set("effort", e.target.value)} />
      </FormField>
      <FormField label="精算幅 下限(h)">
        <FormInput type="number" min={0} value={value.lower_limit_hours} onChange={(e) => set("lower_limit_hours", e.target.value)} />
      </FormField>
      <FormField label="精算幅 上限(h)">
        <FormInput type="number" min={0} value={value.upper_limit_hours} onChange={(e) => set("upper_limit_hours", e.target.value)} />
      </FormField>
      <FormField label="控除単価(円/h)">
        <FormInput type="number" min={0} value={value.deduction_rate} onChange={(e) => set("deduction_rate", e.target.value)} />
      </FormField>
      <FormField label="超過単価(円/h)">
        <FormInput type="number" min={0} value={value.overtime_rate} onChange={(e) => set("overtime_rate", e.target.value)} />
      </FormField>
    </div>
  );
}

const toneClass = {
  ok: "border-emerald-500/40 text-emerald-400",
  warn: "border-amber-500/40 text-amber-400",
  muted: "border-border text-muted-foreground",
} as const;

/** 利益チェック(サーバーの試算結果をそのまま表示する。画面では計算しない) */
function ProfitCheck({ preview, loading, error }: { preview: AssignmentPreview | undefined; loading: boolean; error: string | null }) {
  if (error) {
    return <p className="text-sm text-red-400">試算できませんでした: {error}</p>;
  }
  if (!preview) {
    return (
      <p className="text-sm text-muted-foreground">
        {loading ? "試算しています…" : "単価と精算幅を入力すると、利益を試算します"}
      </p>
    );
  }
  const t = targetBadge(preview.target.status, preview.target.diff_pct, preview.target.target_pct);
  const m = preview.monthly;
  return (
    <div className={`space-y-3 ${loading ? "opacity-60" : ""}`} aria-busy={loading}>
      <dl className="grid grid-cols-3 gap-2 text-sm">
        <div><dt className="text-xs text-muted-foreground">月額売上</dt><dd className="font-medium">{formatYen(m.revenue)}</dd></div>
        <div><dt className="text-xs text-muted-foreground">月額原価</dt><dd className="font-medium">{m.cost === null ? "—(自社社員)" : formatYen(m.cost)}</dd></div>
        <div>
          <dt className="text-xs text-muted-foreground">粗利</dt>
          <dd className={`font-semibold ${m.gross_profit !== null && m.gross_profit < 0 ? "text-red-400" : ""}`}>
            {m.gross_profit === null ? "—" : `${formatYen(m.gross_profit)}(${formatPct(m.margin_pct)})`}
          </dd>
        </div>
      </dl>
      <span className={`inline-block rounded border px-2 py-0.5 text-xs ${toneClass[t.tone]}`}>{t.text}</span>

      {preview.warnings.length > 0 && (
        <ul className="space-y-1" aria-label="警告">
          {preview.warnings.map((w) => (
            <li
              key={w.code + w.message}
              className={`rounded border px-2 py-1 text-xs ${warningTone(w.code) === "danger" ? "border-red-500/50 bg-red-500/10 text-red-300" : "border-amber-500/40 bg-amber-500/10 text-amber-300"}`}
            >
              {w.message}
            </li>
          ))}
        </ul>
      )}

      <table className="w-full text-xs">
        <thead>
          <tr className="text-left text-muted-foreground"><th className="py-1">稼働時間</th><th>売上</th><th>原価</th><th>粗利率</th></tr>
        </thead>
        <tbody>
          {preview.hours_scenarios.map((s) => (
            <tr key={s.label} className="border-t border-border">
              <td className="py-1">{s.label}</td>
              <td>{formatYen(s.revenue)}</td>
              <td>{s.cost === null ? "—" : formatYen(s.cost)}</td>
              <td>{formatPct(s.margin_pct)}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <div className="rounded border border-border p-2 text-xs">
        <p className="mb-1 text-muted-foreground">この案件全体(この要員を加えると)</p>
        <p>
          粗利 {formatYen(preview.project_total.before.gross_profit)}({formatPct(preview.project_total.before.margin_pct)})
          {" → "}
          <span className="font-semibold">
            {formatYen(preview.project_total.after.gross_profit)}({formatPct(preview.project_total.after.margin_pct)})
          </span>
        </p>
      </div>
    </div>
  );
}

export function AssignmentPanel({
  form, setForm, engineerName, projectLabel, partners, preview, previewLoading, previewError, creating, onCreate, onClose,
}: Props) {
  const update = (patch: Partial<AssignmentForm>) => setForm((f) => (f ? { ...f, ...patch } : f));
  const problem = validateForm(form);
  const dirty = form.client.base_rate !== "" || (form.partner?.base_rate ?? "") !== "";

  return (
    <SidePanel
      open
      title={`アサイン作成: ${engineerName}`}
      onClose={onClose}
      dirty={dirty}
      footer={
        <div className="space-y-2">
          {problem && <p className="text-xs text-muted-foreground">{problem}</p>}
          {preview?.monthly.gross_profit != null && preview.monthly.gross_profit < 0 && (
            <p className="text-xs text-red-300">赤字ですが、作成できます。</p>
          )}
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onClose}>キャンセル</Button>
            <Button onClick={onCreate} disabled={creating || problem !== null}>
              {creating ? "作成中…" : "アサイン作成"}
            </Button>
          </div>
        </div>
      }
    >
      <div className="space-y-5">
        <div>
          <p className="text-xs text-muted-foreground">案件</p>
          <p className="text-sm font-medium">{projectLabel}</p>
        </div>

        <div className="grid grid-cols-2 gap-3">
          <FormField label="開始日" required>
            <FormInput type="date" value={form.start_date} onChange={(e) => update({ start_date: e.target.value })} />
          </FormField>
          <FormField label="終了日" required>
            <FormInput type="date" value={form.end_date} onChange={(e) => update({ end_date: e.target.value })} />
          </FormField>
        </div>

        <section className="space-y-2">
          <h3 className="text-sm font-semibold">受注条件</h3>
          <TermsEditor value={form.client} onChange={(client) => update({ client })} />
        </section>

        {form.staff_type === "PARTNER" && form.partner && (
          <section className="space-y-2">
            <h3 className="text-sm font-semibold">発注条件(提案元パートナー)</h3>
            <FormField label="提案元パートナー" required>
              <FormSelect
                value={form.partner.partner_id}
                onChange={(e) => update({ partner: { ...form.partner!, partner_id: e.target.value } })}
                placeholder="パートナーを選択"
                options={partners.map((p) => ({ value: p.partner_id, label: p.name }))}
              />
            </FormField>
            <TermsEditor
              value={form.partner}
              onChange={(t) => update({ partner: { ...form.partner!, ...t } })}
            />
            <FormField label="作業場所">
              <FormInput value={form.partner.work_location_name} onChange={(e) => update({ partner: { ...form.partner!, work_location_name: e.target.value } })} />
            </FormField>
          </section>
        )}

        <section className="space-y-2">
          <h3 className="text-sm font-semibold">利益チェック</h3>
          <ProfitCheck preview={preview} loading={previewLoading} error={previewError} />
        </section>
      </div>
    </SidePanel>
  );
}
