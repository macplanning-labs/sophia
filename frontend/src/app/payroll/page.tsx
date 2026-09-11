"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { getStatus } from "@/lib/status";
import { fetchPayroll, calculatePayroll } from "@/lib/api";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { cn } from "@/lib/utils";
import PayrollDetailPage from "./[id]/client";

const PAYROLL_STATUS_OPTIONS = [
  { value: "DRAFT", label: "計算済" },
  { value: "CONFIRMED", label: "確認済" },
  { value: "PAID", label: "振込済" },
];

export default function PayrollPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <PayrollPageContent />
    </Suspense>
  );
}

function PayrollPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [editId, setEditId] = useState<string | null>(null);
  const [nameFilter, setNameFilter] = useState("");
  const [monthFilter, setMonthFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");

  const queryClient = useQueryClient();
  const { canViewAllPayroll } = useCurrentUser();
  const [calcMonth, setCalcMonth] = useState(() => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
  });

  const calculateMut = useMutation({
    mutationFn: (month: string) => calculatePayroll(month),
    onSuccess: (res) => {
      if (res.success) {
        queryClient.invalidateQueries({ queryKey: ["payroll"] });
        toast.success(res.message ?? "給与計算が完了しました");
      } else {
        toast.error(res.error ?? "給与計算に失敗しました");
      }
    },
    onError: (e: Error) => toast.error(`給与計算に失敗しました: ${e.message}`),
  });

  const { data, isLoading } = useQuery({
    queryKey: ["payroll"],
    queryFn: () => fetchPayroll(),
  });

  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setEditId(fromUrl);
  }, [searchParams]);

  const openEdit = useCallback((id: string | number) => {
    const idStr = String(id);
    setEditId(idStr);
    router.replace(`/payroll?edit=${encodeURIComponent(idStr)}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/payroll", { scroll: false });
  }, [router]);

  const payrolls = data?.payrolls ?? [];

  const nameOptions = useMemo(() => {
    const map = new Map<string, string>();
    for (const p of payrolls) {
      const name = `${p.last_name}${p.first_name}`;
      const key = String(p.employee_id);
      if (!map.has(key)) map.set(key, name);
    }
    return [...map.entries()]
      .sort((a, b) => a[1].localeCompare(b[1], "ja"))
      .map(([value, label]) => ({ value, label }));
  }, [payrolls]);

  const monthOptions = useMemo(() => {
    const months = new Set<string>();
    for (const p of payrolls) {
      const ym = p.year_month?.slice(0, 7);
      if (ym) months.add(ym);
    }
    return [...months]
      .sort((a, b) => b.localeCompare(a))
      .map((ym) => {
        const [y, m] = ym.split("-");
        return { value: ym, label: `${y}年${Number(m)}月` };
      });
  }, [payrolls]);

  const statusOptions = useMemo(() => {
    const present = new Set(payrolls.map((p) => p.status));
    return PAYROLL_STATUS_OPTIONS.filter((o) => present.has(o.value));
  }, [payrolls]);

  const filteredPayrolls = useMemo(
    () =>
      payrolls.filter((p) => {
        if (nameFilter && String(p.employee_id) !== nameFilter) return false;
        if (monthFilter && p.year_month?.slice(0, 7) !== monthFilter) return false;
        if (statusFilter && p.status !== statusFilter) return false;
        return true;
      }),
    [payrolls, nameFilter, monthFilter, statusFilter]
  );

  const summary = useMemo(
    () => ({
      total_count: filteredPayrolls.length,
      total_gross: filteredPayrolls.reduce((s, p) => s + p.gross_pay, 0),
      total_deduction: filteredPayrolls.reduce((s, p) => s + p.deduction_total, 0),
      total_net: filteredPayrolls.reduce((s, p) => s + p.net_pay, 0),
    }),
    [filteredPayrolls]
  );

  return (
    <div className="p-6 space-y-6">
      {canViewAllPayroll && (
        <div className="flex items-center justify-end gap-2">
          <input
            type="month"
            value={calcMonth}
            onChange={(e) => setCalcMonth(e.target.value)}
            className="bg-muted border border-border text-sm text-foreground rounded-md px-2 py-1.5"
          />
          <button
            onClick={() => {
              if (confirm(`${calcMonth}の給与を計算しますか？（既に計算済みの社員はスキップされます）`)) {
                calculateMut.mutate(calcMonth);
              }
            }}
            disabled={calculateMut.isPending}
            className="px-3 py-1.5 text-xs font-medium rounded-md bg-teal-500/10 text-teal-400 border border-teal-500/30 hover:bg-teal-500/20 transition-colors disabled:opacity-50"
          >
            {calculateMut.isPending ? "計算中…" : "💰 給与計算を実行"}
          </button>
        </div>
      )}

      {/* サマリー（表示中フィルタ結果に連動） */}
      <div className="grid grid-cols-4 gap-3">
        {[
          { label: "件数", value: summary.total_count, fmt: false },
          { label: "総支給額", value: summary.total_gross, fmt: true },
          { label: "総控除額", value: summary.total_deduction, fmt: true },
          { label: "差引支給額", value: summary.total_net, fmt: true },
        ].map((s) => (
          <Card key={s.label} className="bg-card border-border">
            <CardContent className="p-3">
              <p className="text-[11px] text-muted-foreground">{s.label}</p>
              <p className="text-xl font-bold tabular-nums text-foreground">
                {s.fmt ? `¥${s.value.toLocaleString()}` : s.value}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">社員コード</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="氏名"
                    options={nameOptions}
                    value={nameFilter}
                    onChange={setNameFilter}
                    placeholder="氏名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="対象月"
                    options={monthOptions}
                    value={monthFilter}
                    onChange={setMonthFilter}
                    placeholder="対象月で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="ステータス"
                    options={statusOptions}
                    value={statusFilter}
                    onChange={setStatusFilter}
                    placeholder="ステータスで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">総支給額</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">控除合計</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">差引支給額</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {payrolls.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredPayrolls.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredPayrolls.map((p) => {
                const st = getStatus("payroll", p.status);
                return (
                  <TableRow key={p.id} className="border-border/50 cursor-pointer hover:bg-accent/50" onClick={() => openEdit(p.id)}>
                    <TableCell className="text-sm font-mono text-muted-foreground">{p.employee_code}</TableCell>
                    <TableCell className="text-sm font-medium">{p.last_name}{p.first_name}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{p.year_month?.slice(0, 7)}</TableCell>
                    <TableCell><Badge variant="outline" className={cn("text-[10px]", st.className)}>{st.label}</Badge></TableCell>
                    <TableCell className="text-right text-sm tabular-nums">¥{p.gross_pay.toLocaleString()}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums text-red-400">-¥{p.deduction_total.toLocaleString()}</TableCell>
                    <TableCell className="text-right text-sm tabular-nums font-medium text-emerald-400">¥{p.net_pay.toLocaleString()}</TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </div>

      {editId && (
        <DetailModal open title={`給与明細 #${editId}`} icon="💰" size="xl" onClose={closeEdit}>
          <PayrollDetailPage id={editId} embedded />
        </DetailModal>
      )}
    </div>
  );
}
