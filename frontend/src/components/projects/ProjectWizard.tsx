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
  const [ccEngineer, setCcEngineer] = useState<SelectOrCreateValue>(emptySelectOrCreate());
  const [ccFields, setCcFields] = useState<ContractFields>({ ...EMPTY_CONTRACT_FIELDS });

  // ③ パートナー契約(0件以上)
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
    mutationFn: () => {
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
        client_contract: skipClientContract ? null : {
          engineer: selectOrCreateToRef(ccEngineer),
          ...contractFieldsToPayload(ccFields),
        },
        partner_contracts: partnerRows.map((row) => ({
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
  const canProceedStep2 = skipClientContract || (
    (ccEngineer.kind === "existing" ? ccEngineer.id !== "" : (ccEngineer.fields.name ?? "").trim() !== "")
    && ccFields.start_date !== "" && ccFields.end_date !== "" && Number(ccFields.base_rate) > 0
  );

  const addPartnerRow = () =>
    setPartnerRows((rows) => [...rows, newPartnerContractRow(workLocationOptions)]);
  const removePartnerRow = (key: string) => setPartnerRows((rows) => rows.filter((r) => r.key !== key));
  const updatePartnerRow = (key: string, patch: Partial<PartnerContractRow>) =>
    setPartnerRows((rows) => rows.map((r) => (r.key === key ? { ...r, ...patch } : r)));

  return (
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <div>
        <h1 className="text-xl font-bold text-foreground">案件 新規作成</h1>
        <p className="text-xs text-muted-foreground mt-1">案件情報 → クライアント契約 → パートナー契約の順に入力します</p>
      </div>

      {/* ステップインジケータ */}
      <div className="flex items-center gap-2 text-xs">
        {[
          { n: 1, label: "案件情報" },
          { n: 2, label: "クライアント契約" },
          { n: 3, label: "パートナー契約" },
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
          <>
            <label className="flex items-center gap-2 text-sm text-foreground">
              <input type="checkbox" checked={skipClientContract} onChange={(e) => setSkipClientContract(e.target.checked)} />
              この案件にはクライアント契約を今は登録しない(後で受注契約ページから登録します)
            </label>
            {!skipClientContract && (
              <div className="space-y-4 pt-2">
                <FormField label="エンジニア(自社社員)" required>
                  <SelectOrCreate
                    options={engineerOptions.map((e: { value: number; label: string }) => ({ value: String(e.value), label: e.label }))}
                    value={ccEngineer}
                    onChange={setCcEngineer}
                    newFields={engineerNewFields(partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label })))}
                  />
                </FormField>
                <div className="grid grid-cols-2 gap-4">
                  <FormField label="開始日" required>
                    <FormInput type="date" value={ccFields.start_date} onChange={(e) => setCcFields({ ...ccFields, start_date: e.target.value })} />
                  </FormField>
                  <FormField label="終了日" required>
                    <FormInput type="date" value={ccFields.end_date} onChange={(e) => setCcFields({ ...ccFields, end_date: e.target.value })} />
                  </FormField>
                </div>
                <div className="grid grid-cols-4 gap-4">
                  <FormField label="精算方式" required>
                    <FormSelect options={SETTLEMENT_TYPES} value={ccFields.settlement_type} onChange={(e) => setCcFields(calcRates({ ...ccFields, settlement_type: e.target.value }))} />
                  </FormField>
                  <FormField label="単金" required>
                    <FormInput type="number" value={ccFields.base_rate} onChange={(e) => setCcFields(calcRates({ ...ccFields, base_rate: e.target.value }))} />
                  </FormField>
                  <FormField label="下限時間">
                    <FormInput type="number" value={ccFields.lower_limit_hours} onChange={(e) => setCcFields(calcRates({ ...ccFields, lower_limit_hours: e.target.value }))} />
                  </FormField>
                  <FormField label="上限時間">
                    <FormInput type="number" value={ccFields.upper_limit_hours} onChange={(e) => setCcFields(calcRates({ ...ccFields, upper_limit_hours: e.target.value }))} />
                  </FormField>
                </div>
                <FormField label="工数">
                  <FormInput type="number" step="0.1" value={ccFields.effort} onChange={(e) => setCcFields({ ...ccFields, effort: e.target.value })} />
                </FormField>
                <FormField label="備考">
                  <FormInput value={ccFields.remarks} onChange={(e) => setCcFields({ ...ccFields, remarks: e.target.value })} />
                </FormField>
              </div>
            )}
          </>
        )}

        {step === 3 && (
          <div className="space-y-6">
            {partnerRows.length === 0 && (
              <p className="text-sm text-muted-foreground">パートナー契約はまだ追加されていません。自社エンジニアのみで回す案件の場合はそのまま作成できます。</p>
            )}
            {partnerRows.map((row, idx) => (
              <div key={row.key} className="border border-border rounded-lg p-4 space-y-4 relative">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-medium text-muted-foreground">パートナー契約 #{idx + 1}</span>
                  <button type="button" onClick={() => removePartnerRow(row.key)} className="text-muted-foreground hover:text-red-400">
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
                <FormField label="パートナー" required>
                  <FormSelect options={partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label }))} placeholder="選択..." value={row.partner_id} onChange={(e) => updatePartnerRow(row.key, { partner_id: e.target.value })} />
                </FormField>
                <FormField label="技術者" required>
                  <SelectOrCreate
                    options={engineerOptions.map((e: { value: number; label: string }) => ({ value: String(e.value), label: e.label }))}
                    value={row.engineer}
                    onChange={(v) => updatePartnerRow(row.key, { engineer: v })}
                    newFields={engineerNewFields(partnerOptions.map((p: { value: string; label: string }) => ({ value: p.value, label: p.label })))}
                  />
                </FormField>
                <FormField label="作業場所">
                  <SelectOrCreate
                    options={workLocationOptions.map((w: { value: number; label: string }) => ({ value: String(w.value), label: w.label }))}
                    value={row.work_location}
                    onChange={(v) => updatePartnerRow(row.key, { work_location: v })}
                    newFields={workLocationNewFields}
                  />
                </FormField>
                <div className="grid grid-cols-2 gap-4">
                  <FormField label="開始日" required>
                    <FormInput type="date" value={row.fields.start_date} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, start_date: e.target.value } })} />
                  </FormField>
                  <FormField label="終了日" required>
                    <FormInput type="date" value={row.fields.end_date} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, end_date: e.target.value } })} />
                  </FormField>
                </div>
                <div className="grid grid-cols-4 gap-4">
                  <FormField label="精算方式" required>
                    <FormSelect options={SETTLEMENT_TYPES} value={row.fields.settlement_type} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, settlement_type: e.target.value }) })} />
                  </FormField>
                  <FormField label="単金" required>
                    <FormInput type="number" value={row.fields.base_rate} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, base_rate: e.target.value }) })} />
                  </FormField>
                  <FormField label="下限時間">
                    <FormInput type="number" value={row.fields.lower_limit_hours} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, lower_limit_hours: e.target.value }) })} />
                  </FormField>
                  <FormField label="上限時間">
                    <FormInput type="number" value={row.fields.upper_limit_hours} onChange={(e) => updatePartnerRow(row.key, { fields: calcRates({ ...row.fields, upper_limit_hours: e.target.value }) })} />
                  </FormField>
                </div>
                <FormField label="工数">
                  <FormInput type="number" step="0.1" value={row.fields.effort} onChange={(e) => updatePartnerRow(row.key, { fields: { ...row.fields, effort: e.target.value } })} />
                </FormField>
              </div>
            ))}
            <Button variant="outline" size="sm" onClick={addPartnerRow} className="gap-1 border-border">
              <Plus className="w-4 h-4" /> パートナー契約を追加
            </Button>
          </div>
        )}

        {createMutation.isError && (
          <p className="text-sm text-red-400">エラー: {(createMutation.error as Error).message}</p>
        )}
      </div>

      <div className="flex justify-between">
        <Button variant="outline" className="border-border" disabled={step === 1} onClick={() => setStep((s) => (s - 1) as 1 | 2 | 3)}>
          戻る
        </Button>
        {step < 3 ? (
          <Button
            className="bg-blue-600 hover:bg-blue-700"
            disabled={(step === 1 && !canProceedStep1) || (step === 2 && !canProceedStep2)}
            onClick={() => setStep((s) => (s + 1) as 1 | 2 | 3)}
          >
            次へ
          </Button>
        ) : (
          <Button className="bg-blue-600 hover:bg-blue-700" disabled={createMutation.isPending} onClick={() => createMutation.mutate()}>
            {createMutation.isPending ? "作成中..." : "作成する"}
          </Button>
        )}
      </div>
    </div>
  );
}
