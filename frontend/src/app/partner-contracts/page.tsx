"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { fetchPartnerContracts, fetchPartnerContractFormData, apiPost } from "@/lib/api";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table";
import { Plus } from "lucide-react";
import { PartnerContractEditModal } from "@/components/contracts/partner-contract-edit-modal";
import {
  WorkLocationField,
  DEFAULT_WORK_LOCATION,
} from "@/components/contracts/work-location-field";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";

interface FormState {
  project_id: string;
  engineer_id: string;
  partner_id: string;
  start_date: string;
  end_date: string;
  settlement_type: string;
  lower_limit_hours: string;
  upper_limit_hours: string;
  fixed_hours: string;
  base_rate: string;
  deduction_rate: string;
  overtime_rate: string;
  effort: string;
  work_location: string;
  remarks: string;
}

const EMPTY_FORM: FormState = {
  project_id: "", engineer_id: "", partner_id: "",
  start_date: "", end_date: "",
  settlement_type: "上下割", lower_limit_hours: "140",
  upper_limit_hours: "180", fixed_hours: "",
  base_rate: "", deduction_rate: "0", overtime_rate: "0",
  effort: "1.0", work_location: DEFAULT_WORK_LOCATION, remarks: "",
};

const SETTLEMENT_TYPES = [
  { value: "上下割", label: "上下割" },
  { value: "中間割", label: "中間割" },
  { value: "固定時間", label: "固定時間" },
  { value: "一律割", label: "一律割" },
];

export default function PartnerContractsPage() {
  return (
    <Suspense fallback={<div className="p-8 text-sm text-muted-foreground">読み込み中...</div>}>
      <PartnerContractsPageContent />
    </Suspense>
  );
}

function PartnerContractsPageContent() {
  const router = useRouter();
  const qc = useQueryClient();
  const searchParams = useSearchParams();
  const prefillProjectId = searchParams.get("project_id");
  const editFromUrl = searchParams.get("edit");

  const { data: contracts, isLoading, isError, error } = useQuery({
    queryKey: ["partner-contracts"],
    queryFn: fetchPartnerContracts,
  });

  const [createOpen, setCreateOpen] = useState(!!prefillProjectId && !editFromUrl);
  const [editId, setEditId] = useState<number | null>(
    editFromUrl && !Number.isNaN(Number(editFromUrl)) ? Number(editFromUrl) : null
  );
  const [form, setForm] = useState<FormState>(
    prefillProjectId ? { ...EMPTY_FORM, project_id: prefillProjectId } : EMPTY_FORM
  );
  const [partnerFilter, setPartnerFilter] = useState("");
  const [projectFilter, setProjectFilter] = useState("");
  const [engineerFilter, setEngineerFilter] = useState("");

  useEffect(() => {
    if (editFromUrl && !Number.isNaN(Number(editFromUrl))) {
      setEditId(Number(editFromUrl));
      setCreateOpen(false);
    }
  }, [editFromUrl]);

  const { data: formData } = useQuery({
    queryKey: ["partner-contract-form-data"],
    queryFn: fetchPartnerContractFormData,
    enabled: createOpen,
  });

  const createMutation = useMutation({
    mutationFn: (data: FormState) => apiPost("/api/v1/partner-contracts", {
      ...data,
      engineer_id: Number(data.engineer_id),
      base_rate: Number(data.base_rate),
      deduction_rate: Number(data.deduction_rate),
      overtime_rate: Number(data.overtime_rate),
    }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["partner-contracts"] });
      setCreateOpen(false);
      setForm(EMPTY_FORM);
    },
  });

  const calcRates = (state: FormState): FormState => {
    const base = Number(state.base_rate) || 0;
    const lower = Number(state.lower_limit_hours) || 0;
    const upper = Number(state.upper_limit_hours) || 0;
    const type = state.settlement_type;

    if (type === "上下割" && lower > 0 && upper > 0 && base > 0) {
      return {
        ...state,
        deduction_rate: String(Math.floor(base / lower / 10) * 10),
        overtime_rate: String(Math.floor(base / upper / 10) * 10),
      };
    } else if (type === "中間割" && lower > 0 && upper > 0 && base > 0) {
      const mid = (lower + upper) / 2;
      const rate = Math.floor(base / mid / 10) * 10;
      return { ...state, deduction_rate: String(rate), overtime_rate: String(rate) };
    } else if (type === "固定時間") {
      return { ...state, deduction_rate: "0", overtime_rate: "0" };
    }
    return state;
  };

  const setField = (key: keyof FormState, value: string) =>
    setForm((prev) => {
      const next = { ...prev, [key]: value };
      if (["base_rate", "settlement_type", "lower_limit_hours", "upper_limit_hours"].includes(key)) {
        return calcRates(next);
      }
      return next;
    });

  const openCreate = () => {
    setForm(EMPTY_FORM);
    setCreateOpen(true);
  };

  const openEdit = useCallback((id: number) => {
    setEditId(id);
    router.replace(`/partner-contracts?edit=${id}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/partner-contracts", { scroll: false });
  }, [router]);

  const workSuggestions = ((formData?.work_locations ?? []) as { label: string }[]).map(
    (w) => w.label
  );

  const partnerOptions = useMemo(() => {
    const names = new Set<string>();
    for (const c of contracts ?? []) {
      if (c.partner_name) names.add(c.partner_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [contracts]);

  const projectOptions = useMemo(() => {
    const names = new Set<string>();
    for (const c of contracts ?? []) {
      if (c.project_name) names.add(c.project_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [contracts]);

  const engineerOptions = useMemo(() => {
    const names = new Set<string>();
    for (const c of contracts ?? []) {
      if (c.engineer_name) names.add(c.engineer_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [contracts]);

  const filteredContracts = useMemo(
    () =>
      (contracts ?? []).filter((c) => {
        if (partnerFilter && c.partner_name !== partnerFilter) return false;
        if (projectFilter && c.project_name !== projectFilter) return false;
        if (engineerFilter && c.engineer_name !== engineerFilter) return false;
        return true;
      }),
    [contracts, partnerFilter, projectFilter, engineerFilter]
  );

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <Button size="sm" className="bg-blue-600 hover:bg-blue-700 text-foreground gap-1" onClick={openCreate}>
          <Plus className="w-4 h-4" /> 新規作成
        </Button>
      </div>

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">
            {[...Array(4)].map((_, i) => (
              <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />
            ))}
          </div>
        ) : isError ? (
          <div className="p-8 text-center text-sm text-red-400">
            発注契約の取得に失敗しました: {(error as Error).message}
          </div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">ID</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="パートナー"
                    options={partnerOptions}
                    value={partnerFilter}
                    onChange={setPartnerFilter}
                    placeholder="パートナー名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="案件"
                    options={projectOptions}
                    value={projectFilter}
                    onChange={setProjectFilter}
                    placeholder="案件名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="作業者"
                    options={engineerOptions}
                    value={engineerFilter}
                    onChange={setEngineerFilter}
                    placeholder="作業者名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">精算方式</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">単金</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">工数</TableHead>
                <TableHead className="text-xs text-muted-foreground">期間</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">状態</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(contracts ?? []).length === 0 ? (
                <TableRow>
                  <TableCell colSpan={9} className="text-center py-12 text-muted-foreground">
                    データがありません
                  </TableCell>
                </TableRow>
              ) : filteredContracts.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={9} className="text-center py-12 text-muted-foreground">
                    該当するデータがありません
                  </TableCell>
                </TableRow>
              ) : (
                filteredContracts.map((c) => (
                  <TableRow
                    key={c.id}
                    className="border-border/50 cursor-pointer hover:bg-accent/50"
                    onClick={() => openEdit(c.id)}
                  >
                    <TableCell className="text-sm tabular-nums text-muted-foreground">{c.id}</TableCell>
                    <TableCell className="text-sm font-medium">{c.partner_name}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{c.project_name}</TableCell>
                    <TableCell className="text-sm">{c.engineer_name}</TableCell>
                    <TableCell><StatusBadge status={c.settlement_type} label={c.settlement_type} /></TableCell>
                    <TableCell className="text-right text-sm tabular-nums">
                      ¥{c.base_rate.toLocaleString()}
                    </TableCell>
                    <TableCell className="text-right text-sm tabular-nums text-muted-foreground">
                      {c.effort}
                    </TableCell>
                    <TableCell className="text-[11px] text-muted-foreground">
                      {c.start_date} ~ {c.end_date}
                    </TableCell>
                    <TableCell className="text-center">
                      <StatusBadge status={c.is_active ? "ACTIVE" : "INACTIVE"} />
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">
            表示中: {filteredContracts.length}件
            {(partnerFilter || projectFilter || engineerFilter) && contracts
              ? ` / ${contracts.length}件`
              : ""}
          </span>
        </div>
      </div>

      <FormModal
        open={createOpen}
        title="発注契約 新規作成"
        size="lg"
        loading={createMutation.isPending}
        onSubmit={() => createMutation.mutate(form)}
        onClose={() => setCreateOpen(false)}
      >
        <div className="grid grid-cols-3 gap-4">
          <FormField label="パートナー" required>
            <FormSelect
              options={formData?.partners ?? []}
              placeholder="選択..."
              value={form.partner_id}
              onChange={(e) => setField("partner_id", e.target.value)}
            />
          </FormField>
          <FormField label="案件" required>
            <FormSelect
              options={formData?.projects ?? []}
              placeholder="選択..."
              value={form.project_id}
              onChange={(e) => setField("project_id", e.target.value)}
            />
          </FormField>
          <FormField label="エンジニア" required>
            <FormSelect
              options={(formData?.engineers ?? []).map((e: { value: number; label: string }) => ({ value: String(e.value), label: e.label }))}
              placeholder="選択..."
              value={form.engineer_id}
              onChange={(e) => setField("engineer_id", e.target.value)}
            />
          </FormField>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <FormField label="開始日" required>
            <FormInput type="date" value={form.start_date} onChange={(e) => setField("start_date", e.target.value)} />
          </FormField>
          <FormField label="終了日" required>
            <FormInput type="date" value={form.end_date} onChange={(e) => setField("end_date", e.target.value)} />
          </FormField>
        </div>

        <div className="grid grid-cols-4 gap-4">
          <FormField label="精算方式" required>
            <FormSelect options={SETTLEMENT_TYPES} value={form.settlement_type} onChange={(e) => setField("settlement_type", e.target.value)} />
          </FormField>
          <FormField label="単金" required>
            <FormInput type="number" value={form.base_rate} onChange={(e) => setField("base_rate", e.target.value)} placeholder="350000" />
          </FormField>
          <FormField label="下限時間">
            <FormInput type="number" value={form.lower_limit_hours} onChange={(e) => setField("lower_limit_hours", e.target.value)} />
          </FormField>
          <FormField label="上限時間">
            <FormInput type="number" value={form.upper_limit_hours} onChange={(e) => setField("upper_limit_hours", e.target.value)} />
          </FormField>
        </div>

        <div className="grid grid-cols-3 gap-4">
          <FormField label="工数">
            <FormInput type="number" step="0.1" value={form.effort} onChange={(e) => setField("effort", e.target.value)} />
          </FormField>
          <FormField label="控除単価">
            <FormInput type="number" value={form.deduction_rate} onChange={(e) => setField("deduction_rate", e.target.value)} />
          </FormField>
          <FormField label="超過単価">
            <FormInput type="number" value={form.overtime_rate} onChange={(e) => setField("overtime_rate", e.target.value)} />
          </FormField>
        </div>

        <WorkLocationField
          value={form.work_location}
          onChange={(v) => setField("work_location", v)}
          suggestions={workSuggestions}
        />

        <FormField label="備考">
          <FormInput value={form.remarks} onChange={(e) => setField("remarks", e.target.value)} placeholder="自由記述" />
        </FormField>

        {createMutation.isError && (
          <p className="text-sm text-red-400">
            エラー: {(createMutation.error as Error).message}
          </p>
        )}
      </FormModal>

      <PartnerContractEditModal
        contractId={editId}
        open={editId != null}
        onClose={closeEdit}
      />
    </div>
  );
}
