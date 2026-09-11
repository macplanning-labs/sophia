"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchEmployees } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FormModal } from "@/components/ui/form-modal";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Plus } from "lucide-react";
import EmployeeDetailPage from "./[id]/client";

const EMPLOYMENT_TYPE_LABELS: Record<string, string> = {
  REGULAR: "正社員",
  CONTRACT: "契約社員",
  PART_TIME: "パート",
};

export default function EmployeesPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <EmployeesPageContent />
    </Suspense>
  );
}

function EmployeesPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const queryClient = useQueryClient();
  const [editId, setEditId] = useState<string | null>(null);
  const [nameFilter, setNameFilter] = useState("");
  const [typeFilter, setTypeFilter] = useState("");
  const [activeFilter, setActiveFilter] = useState("");

  const { data: employees, isLoading } = useQuery({
    queryKey: ["employees"],
    queryFn: fetchEmployees,
  });

  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setEditId(fromUrl);
  }, [searchParams]);

  const openEdit = useCallback((id: string | number) => {
    const idStr = String(id);
    setEditId(idStr);
    router.replace(`/employees?edit=${encodeURIComponent(idStr)}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/employees", { scroll: false });
  }, [router]);

  const rows = employees ?? [];

  const nameOptions = useMemo(() => {
    const map = new Map<string, string>();
    for (const e of rows) {
      const name = `${e.last_name}${e.first_name}`;
      const key = String(e.id);
      if (!map.has(key)) map.set(key, name);
    }
    return [...map.entries()]
      .sort((a, b) => a[1].localeCompare(b[1], "ja"))
      .map(([value, label]) => ({ value, label, searchText: rows.find((r) => String(r.id) === value)?.employee_id }));
  }, [rows]);

  const typeOptions = useMemo(() => {
    const types = new Set(rows.map((e) => e.employment_type).filter(Boolean));
    return [...types]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((t) => ({ value: t, label: EMPLOYMENT_TYPE_LABELS[t] ?? t }));
  }, [rows]);

  const activeOptions = useMemo(() => {
    const opts: { value: string; label: string }[] = [];
    if (rows.some((e) => e.is_active)) opts.push({ value: "active", label: "在籍" });
    if (rows.some((e) => !e.is_active)) opts.push({ value: "inactive", label: "退職" });
    return opts;
  }, [rows]);

  const filteredEmployees = useMemo(
    () =>
      rows.filter((e) => {
        if (nameFilter && String(e.id) !== nameFilter) return false;
        if (typeFilter && e.employment_type !== typeFilter) return false;
        if (activeFilter === "active" && !e.is_active) return false;
        if (activeFilter === "inactive" && e.is_active) return false;
        return true;
      }),
    [rows, nameFilter, typeFilter, activeFilter]
  );

  // 新規作成モーダル
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({
    employee_id: "",
    last_name: "",
    first_name: "",
    last_name_kana: "",
    first_name_kana: "",
    employment_type: "REGULAR",
    email: "",
    base_salary: 0,
    hire_date: "",
  });

  const createMutation = useMutation({
    mutationFn: async () => {
      const res = await fetch("/api/v1/employees", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify(form),
      });
      if (!res.ok) throw new Error("作成に失敗しました");
      return res.json();
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["employees"] });
      setShowCreate(false);
      setForm({
        employee_id: "", last_name: "", first_name: "",
        last_name_kana: "", first_name_kana: "",
        employment_type: "REGULAR", email: "", base_salary: 0, hire_date: "",
      });
    },
    onError: (err) => {
      alert(err instanceof Error ? err.message : "作成に失敗しました");
    },
  });

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <Button size="sm" onClick={() => setShowCreate(true)} className="gap-1.5">
          <Plus className="w-3.5 h-3.5" /> 新規作成
        </Button>
      </div>
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">社員番号</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="氏名"
                    options={nameOptions}
                    value={nameFilter}
                    onChange={setNameFilter}
                    placeholder="氏名・社員番号で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="雇用形態"
                    options={typeOptions}
                    value={typeFilter}
                    onChange={setTypeFilter}
                    placeholder="雇用形態で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">入社日</TableHead>
                <TableHead className="text-xs text-muted-foreground">メール</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">基本給</TableHead>
                <TableHead className="text-xs text-center">
                  <SearchableColumnHeader
                    label="状態"
                    options={activeOptions}
                    value={activeFilter}
                    onChange={setActiveFilter}
                    placeholder="状態で検索…"
                  />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredEmployees.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredEmployees.map((e) => (
                <TableRow key={e.id} className="border-border/50 cursor-pointer hover:bg-accent/50" onClick={() => openEdit(e.id)}>
                  <TableCell className="text-sm font-mono text-muted-foreground">{e.employee_id}</TableCell>
                  <TableCell className="text-sm font-medium">{e.last_name}{e.first_name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{EMPLOYMENT_TYPE_LABELS[e.employment_type] ?? e.employment_type}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{e.hire_date ?? "—"}</TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">{e.email}</TableCell>
                  <TableCell className="text-right text-sm tabular-nums">¥{e.base_salary.toLocaleString()}</TableCell>
                  <TableCell className="text-center">
                    {e.is_active ? <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">在籍</Badge>
                     : <Badge variant="outline" className="border-border text-muted-foreground text-[10px]">退職</Badge>}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredEmployees.length}件</span>
        </div>
      </div>

      {/* 新規作成モーダル */}
      <FormModal
        open={showCreate}
        title="社員新規作成"
        size="lg"
        loading={createMutation.isPending}
        submitLabel="登録"
        onSubmit={() => createMutation.mutate()}
        onClose={() => setShowCreate(false)}
      >
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="block text-xs text-muted-foreground mb-1">社員番号 *</label>
            <input
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.employee_id}
              onChange={(e) => setForm({ ...form, employee_id: e.target.value })}
              placeholder="EMP001"
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">雇用形態</label>
            <select
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.employment_type}
              onChange={(e) => setForm({ ...form, employment_type: e.target.value })}
            >
              <option value="REGULAR">正社員</option>
              <option value="CONTRACT">契約社員</option>
              <option value="PART_TIME">パート</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">姓 *</label>
            <input
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.last_name}
              onChange={(e) => setForm({ ...form, last_name: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">名 *</label>
            <input
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.first_name}
              onChange={(e) => setForm({ ...form, first_name: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">姓（カナ）</label>
            <input
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.last_name_kana}
              onChange={(e) => setForm({ ...form, last_name_kana: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">名（カナ）</label>
            <input
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.first_name_kana}
              onChange={(e) => setForm({ ...form, first_name_kana: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">メールアドレス</label>
            <input
              type="email"
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.email}
              onChange={(e) => setForm({ ...form, email: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">入社日</label>
            <input
              type="date"
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.hire_date}
              onChange={(e) => setForm({ ...form, hire_date: e.target.value })}
            />
          </div>
          <div>
            <label className="block text-xs text-muted-foreground mb-1">基本給</label>
            <input
              type="number"
              className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
              value={form.base_salary}
              onChange={(e) => setForm({ ...form, base_salary: parseInt(e.target.value) || 0 })}
            />
          </div>
        </div>
      </FormModal>

      {editId && (
        <DetailModal open title={`社員 #${editId}`} icon="👤" size="xl" onClose={closeEdit}>
          <EmployeeDetailPage id={editId} embedded onDeleted={closeEdit} />
        </DetailModal>
      )}
    </div>
  );
}
