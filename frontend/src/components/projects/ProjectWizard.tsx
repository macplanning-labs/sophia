"use client";

// 案件作成ウィザード(WS3, 2026-07-31)。
// ①案件情報 → ②クライアント契約(スキップ可能) → ③パートナー契約(0〜複数)の3ステップ。
// 各ステップの入力はローカルstateに保持するだけで、最終ステップの「作成する」押下時に
// 1回だけ POST /api/v1/projects/wizard を呼ぶ(バックエンド側は1トランザクション)。
// このコードベース初の複数ステップフォームのため、既存の単一ステップFormModalパターンは使わず
// 専用ページ(/projects/new)にホストする前提で作っている。
//
// 責任者/担当者(甲乙)・deliverable_text・payment_condition・contract_items・締め日設定の細部は
// このウィザードでは扱わない(作成後に既存の /partner-contracts/{id} 編集画面で補完する想定)。

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { fetchProjectWizardFormData, submitProjectWizard } from "@/lib/api";
import { FormField, FormInput, FormSelect, FormTextarea } from "@/components/ui/form-modal";
import { Button } from "@/components/ui/button";
import {
  SelectOrCreate, selectOrCreateToRef, emptySelectOrCreate,
  type SelectOrCreateValue, type NewFieldDef,
} from "@/components/ui/select-or-create";
import { Plus, Trash2 } from "lucide-react";

const SETTLEMENT_TYPES = [
  { value: "上下割", label: "上下割" },
  { value: "中間割", label: "中間割" },
  { value: "固定時間", label: "固定時間" },
  { value: "一律割", label: "一律割" },
];

const REPORT_DEADLINE_TYPES = [
  { value: "RELATIVE", label: "月末相対（N営業日前）" },
  { value: "FIXED_DAY", label: "当月固定日" },
];

const REPORT_DEADLINE_HOLIDAY_RULES = [
  { value: "", label: "（RELATIVEの場合は未使用）" },
  { value: "PREVIOUS_BUSINESS_DAY", label: "前営業日" },
  { value: "NEXT_BUSINESS_DAY", label: "翌営業日" },
];

const AFFILIATION_OPTIONS = [
  { value: "EMPLOYEE", label: "自社社員" },
  { value: "PARTNER", label: "パートナー" },
];

interface ContractFields {
  start_date: string;
  end_date: string;
  settlement_type: string;
  base_rate: string;
  effort: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  deduction_rate: string;
  overtime_rate: string;
  remarks: string;
}

const EMPTY_CONTRACT_FIELDS: ContractFields = {
  start_date: "", end_date: "",
  settlement_type: "上下割",
  base_rate: "", effort: "1.0",
  lower_limit_hours: "140", upper_limit_hours: "180",
  deduction_rate: "0", overtime_rate: "0",
  remarks: "",
};

// 精算単価の自動計算(SES精算ルール§3。client-contracts/page.tsxと同じロジック)
function calcRates(f: ContractFields): ContractFields {
  const base = Number(f.base_rate) || 0;
  const lower = Number(f.lower_limit_hours) || 0;
  const upper = Number(f.upper_limit_hours) || 0;
  if (f.settlement_type === "上下割" && lower > 0 && upper > 0 && base > 0) {
    return { ...f, deduction_rate: String(Math.floor(base / lower / 10) * 10), overtime_rate: String(Math.floor(base / upper / 10) * 10) };
  }
  if (f.settlement_type === "中間割" && lower > 0 && upper > 0 && base > 0) {
    const rate = String(Math.floor(base / ((lower + upper) / 2) / 10) * 10);
    return { ...f, deduction_rate: rate, overtime_rate: rate };
  }
  if (f.settlement_type === "固定時間") {
    return { ...f, deduction_rate: "0", overtime_rate: "0" };
  }
  return f;
}

function contractFieldsToPayload(f: ContractFields) {
  return {
    start_date: f.start_date,
    end_date: f.end_date,
    settlement_type: f.settlement_type,
    base_rate: Number(f.base_rate) || 0,
    effort: Number(f.effort) || 1.0,
    lower_limit_hours: Number(f.lower_limit_hours) || 0,
    upper_limit_hours: Number(f.upper_limit_hours) || 0,
    fixed_hours: null,
    deduction_rate: Number(f.deduction_rate) || 0,
    overtime_rate: Number(f.overtime_rate) || 0,
    mid_month_rule: null,
    remarks: f.remarks || null,
  };
}

interface PartnerContractRow {
  key: string;
  partner_id: string;
  engineer: SelectOrCreateValue;
  work_location: SelectOrCreateValue;
  fields: ContractFields;
}

interface ClientContractRow {
  key: string;
  engineer: SelectOrCreateValue;
  fields: ContractFields;
}

function newClientContractRow(): ClientContractRow {
  return { key: crypto.randomUUID(), engineer: emptySelectOrCreate(), fields: { ...EMPTY_CONTRACT_FIELDS } };
}

// 1行1名の入力行（列幅を行ごとにそろえ、横並びで比べられるようにする）
const CELL = "!px-2 !py-1.5 !text-xs whitespace-nowrap text-ellipsis";
const CELL_DATE = "!pl-2 !pr-8 !py-1.5 !text-xs whitespace-nowrap";
const ROW_HEAD = "grid gap-2 text-[11px] font-medium text-muted-foreground px-0.5 whitespace-nowrap [&>span]:overflow-hidden [&>span]:text-ellipsis";
const CLIENT_COLS = "minmax(200px,1.6fr) 150px 150px 100px 100px 70px 70px 64px minmax(120px,1fr) 32px";
const PARTNER_COLS = "minmax(150px,1fr) minmax(200px,1.4fr) minmax(150px,1fr) 150px 150px 100px 100px 70px 70px 64px 32px";

const DEFAULT_WORK_LOCATION = "弊社指定場所";

function defaultWorkLocation(options: { value: number | string; label: string }[]): SelectOrCreateValue {
  const hit = options.find((o) => o.label === DEFAULT_WORK_LOCATION);
  if (hit) return { kind: "existing", id: String(hit.value) };
  return { kind: "new", fields: { name: DEFAULT_WORK_LOCATION } };
}

function newPartnerContractRow(
  workLocationOptions: { value: number | string; label: string }[] = []
): PartnerContractRow {
  return {
    key: crypto.randomUUID(),
    partner_id: "",
    engineer: emptySelectOrCreate(),
    work_location: defaultWorkLocation(workLocationOptions),
    fields: { ...EMPTY_CONTRACT_FIELDS },
  };
}

export function ProjectWizard() {
  const router = useRouter();
  const qc = useQueryClient();
  const [step, setStep] = useState<1 | 2 | 3>(1);

  const { data: formData } = useQuery({
    queryKey: ["project-wizard-form-data"],
    queryFn: fetchProjectWizardFormData,
  });

  // ① 案件情報
  const [clientId, setClientId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [reportDeadlineType, setReportDeadlineType] = useState("RELATIVE");
  const [reportDeadlineValue, setReportDeadlineValue] = useState("");
  const [reportDeadlineHolidayRule, setReportDeadlineHolidayRule] = useState("");
  const [reportRequestDay, setReportRequestDay] = useState("");

  // ② クライアント契約(スキップ可能)
  const [skipClientContract, setSkipClientContract] = useState(false);
  // 受注契約は技術者ごとに1件。複数人分を入力できる
  const [clientRows, setClientRows] = useState<ClientContractRow[]>([newClientContractRow()]);

  // ③ パートナー契約(スキップ可能。自社社員のみの場合は「スキップして作成」)
  const [partnerRows, setPartnerRows] = useState<PartnerContractRow[]>([]);

  const engineerNewFields = (partnerOptions: { value: string; label: string }[]): NewFieldDef[] => [
    { key: "name", label: "氏名", required: true },
    { key: "name_kana", label: "フリガナ" },
    { key: "affiliation_type", label: "所属", type: "select", options: AFFILIATION_OPTIONS },
    { key: "partner_id", label: "所属パートナー(パートナーの場合)", type: "select", options: partnerOptions },
    { key: "email", label: "メール", type: "email" },
  ];

  const workLocationNewFields: NewFieldDef[] = [{ key: "name", label: "作業場所名", required: true }];

  const engineerOptions = formData?.engineers ?? [];
  const partnerOptions = formData?.partners ?? [];
  const workLocationOptions = formData?.work_locations ?? [];
  const clientOptions = formData?.clients ?? [];

  const createMutation = useMutation({
    mutationFn: (skipPartners: boolean) => {
      const body = {
        project: {
          client_id: Number(clientId),
          name,
          description: description || null,
          report_deadline_type: reportDeadlineType,
          report_deadline_value: reportDeadlineValue ? Number(reportDeadlineValue) : null,
          report_deadline_holiday_rule: reportDeadlineHolidayRule || null,
          report_request_day: reportRequestDay ? Number(reportRequestDay) : null,
        },
        client_contracts: skipClientContract ? [] : clientRows.map((row) => ({
          engineer: selectOrCreateToRef(row.engineer),
          ...contractFieldsToPayload(row.fields),
        })),
        partner_contracts: (skipPartners ? [] : partnerRows).map((row) => ({
          partner_id: row.partner_id,
          engineer: selectOrCreateToRef(row.engineer),
          work_location: selectOrCreateToRef(row.work_location),
          ...contractFieldsToPayload(row.fields),
        })),
      };
      return submitProjectWizard(body);
    },
    onSuccess: (res) => {
      if (!res.success || !res.project_id) {
        toast.error(res.error || "作成に失敗しました");
        return;
      }
      qc.invalidateQueries({ queryKey: ["projects"] });
      toast.success("案件を作成しました");
      router.push(`/projects/${res.project_id}`);
    },
    onError: (e: Error) => toast.error(`作成エラー: ${e.message}`),
  });

  const canProceedStep1 = clientId !== "" && name.trim() !== "";
  const canProceedStep2 = (
    clientRows.length > 0 && clientRows.every((r) =>
      (r.engineer.kind === "existing" ? r.engineer.id !== "" : (r.engineer.fields.name ?? "").trim() !== "")
      && r.fields.start_date !== "" && r.fields.end_date !== "" && Number(r.fields.base_rate) > 0
    )
  );

  const addClientRow = () => setClientRows((rows) => [...rows, newClientContractRow()]);
  const removeClientRow = (key: string) => setClientRows((rows) => rows.filter((r) => r.key !== key));
  const updateClientRow = (key: string, patch: Partial<ClientContractRow>) =>
    setClientRows((rows) => rows.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  const canCreate = (
    partnerRows.length > 0 && partnerRows.every((r) =>
      r.partner_id !== ""
      && (r.engineer.kind === "existing" ? r.engineer.id !== "" : (r.engineer.fields.name ?? "").trim() !== "")
      && r.fields.start_date !== "" && r.fields.end_date !== "" && Number(r.fields.base_rate) > 0
    )
  );

  const addPartnerRow = () =>
    setPartnerRows((rows) => [...rows, newPartnerContractRow(workLocationOptions)]);
  const removePartnerRow = (key: string) => setPartnerRows((rows) => rows.filter((r) => r.key !== key));
  const updatePartnerRow = (key: string, patch: Partial<PartnerContractRow>) =>
    setPartnerRows((rows) => rows.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  return (
    <div className={`${step === 1 ? "max-w-3xl" : "max-w-[1400px]"} mx-auto p-6 space-y-6`}>
      <div>
        <h1 className="text-xl font-bold text-foreground">案件 新規作成</h1>
        <p className="text-xs text-muted-foreground mt-1">案件情報 → 受注契約 → 発注契約の順に入力します。受注契約・発注契約は、技術者ごとに1行ずつ入力します</p>
      </div>

      {/* ステップインジケータ */}
      <div className="flex items-center gap-2 text-xs">
        {[
          { n: 1, label: "案件情報" },
          { n: 2, label: "受注契約" },
          { n: 3, label: "発注契約(パートナー)" },
        ].map((s, i) => (
          <div key={s.n} className="flex items-center gap-2">
            <span
              className={`w-6 h-6 rounded-full flex items-center justify-center font-medium ${
                step === s.n ? "bg-blue-600 text-white" : step > s.n ? "bg-emerald-600/30 text-emerald-400" : "bg-muted text-muted-foreground"
              }`}
            >
              {s.n}
            </span>
            <span className={step === s.n ? "text-foreground font-medium" : "text-muted-foreground"}>{s.label}</span>
            {i < 2 && <span className="text-muted-foreground mx-1">→</span>}
          </div>
        ))}
      </div>

      <div className="bg-card border border-border rounded-lg p-6 space-y-4">
        {step === 1 && (
          <>
            <FormField label="クライアント" required>
              <FormSelect options={clientOptions.map((c: { value: number; label: string }) => ({ value: String(c.value), label: c.label }))} placeholder="選択..." value={clientId} onChange={(e) => setClientId(e.target.value)} />
            </FormField>
            <FormField label="案件名" required>
              <FormInput value={name} onChange={(e) => setName(e.target.value)} placeholder="○○社基幹システム刷新" />
            </FormField>
            <FormField label="説明">
              <FormTextarea value={description} onChange={(e) => setDescription(e.target.value)} />
            </FormField>
            <div className="grid grid-cols-2 gap-4">
              <FormField label="稼働報告提出期限の種類" required>
                <FormSelect options={REPORT_DEADLINE_TYPES} value={reportDeadlineType} onChange={(e) => setReportDeadlineType(e.target.value)} />
              </FormField>
              <FormField label="提出期限の値(空欄なら受注契約側の設定に従う)">
                <FormInput type="number" value={reportDeadlineValue} onChange={(e) => setReportDeadlineValue(e.target.value)} />
              </FormField>
            </div>
            {reportDeadlineType === "FIXED_DAY" && (
              <FormField label="非営業日の場合の調整">
                <FormSelect options={REPORT_DEADLINE_HOLIDAY_RULES} value={reportDeadlineHolidayRule} onChange={(e) => setReportDeadlineHolidayRule(e.target.value)} />
              </FormField>
            )}
            <FormField label="稼働報告 初回依頼送信日(クライアント経由でしか稼働報告が届かない案件のみ)">
              <FormInput type="number" value={reportRequestDay} onChange={(e) => setReportRequestDay(e.target.value)} />
            </FormField>
          </>
        )}

        {step === 2 && (
          <div className="space-y-3">
            <div className="overflow-x-auto">
              <div className="min-w-[1100px] space-y-2">
                <div className={ROW_HEAD} style={{ gridTemplateColumns: CLIENT_COLS }}>
                  <span>技術者 *</span><span>開始日 *</span><span>終了日 *</span><span>精算方式 *</span>
                  <span>単金 *</span><span>下限</span><span>上限</span><span>工数</span><span>備考</span><span />
                </div>
                {clientRows.map((row) => (
                  <div key={row.key} className="grid gap-2 items-start" style={{ gridTemplateColumns: CLIENT_COLS }}>
                    <SelectOrCreate
                      options={engineerOptions.map((e: { value: number; label: string }) => ({ value: String(e.value), label: e.label }))}
                      value={row.engineer}
                      onChange={(v) => updateClientRow(row.key, { engineer: v })}
                      newFields={engineerNewFields(partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label })))}
                    />
                    <FormInput className={CELL_DATE} type="date" value={row.fields.start_date} onChange={(e) => updateClientRow(row.key, { fields: { ...row.fields, start_date: e.target.value } })} />
                    <FormInput className={CELL_DATE} type="date" value={row.fields.end_date} onChange={(e) => updateClientRow(row.key, { fields: { ...row.fields, end_date: e.target.value } })} />
                    <FormSelect className={CELL} options={SETTLEMENT_TYPES} value={row.fields.settlement_type} onChange={(e) => updateClientRow(row.key, { fields: calcRates({ ...row.fields, settlement_type: e.target.value }) })} />
                    <FormInput className={CELL} type="number" value={row.fields.base_rate} onChange={(e) => updateClientRow(row.key, { fields: calcRates({ ...row.fields, base_rate: e.target.value }) })} />
                    <FormInput className={CELL} type="number" value={row.fields.lower_limit_hours} onChange={(e) => updateClientRow(row.key, { fields: calcRates({ ...row.fields, lower_limit_hours: e.target.value }) })} />
                    <FormInput className={CELL} type="number" value={row.fields.upper_limit_hours} onChange={(e) => updateClientRow(row.key, { fields: calcRates({ ...row.fields, upper_limit_hours: e.target.value }) })} />
                    <FormInput className={CELL} type="number" step="0.1" value={row.fields.effort} onChange={(e) => updateClientRow(row.key, { fields: { ...row.fields, effort: e.target.value } })} />
                    <FormInput className={CELL} value={row.fields.remarks} onChange={(e) => updateClientRow(row.key, { fields: { ...row.fields, remarks: e.target.value } })} />
                    <button type="button" onClick={() => removeClientRow(row.key)} className="p-1.5 text-muted-foreground hover:text-red-400" aria-label="この行を削除">
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            </div>
            <Button variant="outline" size="sm" onClick={addClientRow} className="gap-1 border-border">
              <Plus className="w-4 h-4" /> 受注契約を追加(1行 = 1名)
            </Button>
          </div>
        )}

        {step === 3 && (
          <div className="space-y-3">
            {partnerRows.length === 0 && (
              <p className="text-sm text-muted-foreground">発注契約を追加してください。自社社員のみの場合は「スキップして作成」を押してください。</p>
            )}
            {partnerRows.length > 0 && (
              <div className="overflow-x-auto">
                <div className="min-w-[1300px] space-y-2">
                  <div className={ROW_HEAD} style={{ gridTemplateColumns: PARTNER_COLS }}>
                    <span>パートナー *</span><span>技術者 *</span><span>作業場所</span><span>開始日 *</span><span>終了日 *</span>
                    <span>精算方式 *</span><span>単金 *</span><span>下限</span><span>上限</span><span>工数</span><span />
                  </div>
                  {partnerRows.map((row) => (
                    <div key={row.key} className="grid gap-2 items-start" style={{ gridTemplateColumns: PARTNER_COLS }}>
                      <FormSelect className={CELL} options={partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label }))} placeholder="選択..." value={row.partner_id} onChange={(e) => updatePartnerRow(row.key, { partner_id: e.target.value })} />
                      <SelectOrCreate
                        options={engineerOptions.map((e: { value: number; label: string }) => ({ value: String(e.value), label: e.label }))}
                        value={row.engineer}
                        onChange={(v) => updatePartnerRow(row.key, { engineer: v })}
                        newFields={engineerNewFields(partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label })))}
                      />
                      <SelectOrCreate
                        options={workLocationOptions.map((w: { value: number; label: string }) => ({ value: String(w.value), label: w.label }))}
                        value={row.work_location}
                        onChange={(v) => updatePartnerRow(row.key, { work_location: v })}
                        newFields={workLocationNewFields}
                      />
                      <FormInput className={CELL_DATE} type="date" value={row.fields.start_date} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, start_date: e.target.value } })} />
                      <FormInput className={CELL_DATE} type="date" value={row.fields.end_date} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, end_date: e.target.value } })} />
                      <FormSelect className={CELL} options={SETTLEMENT_TYPES} value={row.fields.settlement_type} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, settlement_type: e.target.value }) })} />
                      <FormInput className={CELL} type="number" value={row.fields.base_rate} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, base_rate: e.target.value }) })} />
                      <FormInput className={CELL} type="number" value={row.fields.lower_limit_hours} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, lower_limit_hours: e.target.value }) })} />
                      <FormInput className={CELL} type="number" value={row.fields.upper_limit_hours} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, upper_limit_hours: e.target.value }) })} />
                      <FormInput className={CELL} type="number" step="0.1" value={row.fields.effort} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, effort: e.target.value } })} />
                      <button type="button" onClick={() => removePartnerRow(row.key)} className="p-1.5 text-muted-foreground hover:text-red-400" aria-label="この行を削除">
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}
            <Button variant="outline" size="sm" onClick={addPartnerRow} className="gap-1 border-border">
              <Plus className="w-4 h-4" /> 発注契約を追加(1行 = 1名)
            </Button>
          </div>
        )}

        {createMutation.isError && (
          <p className="text-sm text-red-400">エラー: {(createMutation.error as Error).message}</p>
        )}
      </div>

      <div className="flex justify-between">
        <Button
          variant="outline"
          className="border-border"
          disabled={step === 1}
          onClick={() => { setSkipClientContract(false); setStep((s) => (s - 1) as 1 | 2 | 3); }}
        >
          戻る
        </Button>
        <div className="flex gap-2">
          {step === 2 && (
            <Button variant="outline" className="border-border" onClick={() => { setSkipClientContract(true); setStep(3); }}>
              スキップ
            </Button>
          )}
          {step === 3 && (
            <Button variant="outline" className="border-border" disabled={createMutation.isPending} onClick={() => createMutation.mutate(true)}>
              スキップして作成
            </Button>
          )}
          {step < 3 ? (
            <Button
              className="bg-blue-600 hover:bg-blue-700"
              disabled={(step === 1 && !canProceedStep1) || (step === 2 && !canProceedStep2)}
              onClick={() => { setSkipClientContract(false); setStep((s) => (s + 1) as 1 | 2 | 3); }}
            >
              次へ
            </Button>
          ) : (
            <Button className="bg-blue-600 hover:bg-blue-700" disabled={createMutation.isPending || !canCreate} onClick={() => createMutation.mutate(false)}>
              {createMutation.isPending ? "作成中..." : "作成する"}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
