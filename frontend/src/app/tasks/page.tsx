"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { getStatus } from "@/lib/status";
import { fetchTasks } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { PageHeader } from "@/components/ui/page-header";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { useState } from "react";
import { toast } from "sonner";


export default function TasksPage() {
  const queryClient = useQueryClient();
  const [genMonth, setGenMonth] = useState(() => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
  });

  const { data, isLoading } = useQuery({
    queryKey: ["tasks"],
    queryFn: () => fetchTasks(),
  });

  const generateMut = useMutation({
    mutationFn: async (month: string) => {
      const fd = new FormData();
      fd.append("work_month", `${month}-01`);
      const res = await fetch("/api/v1/tasks/generate", { method: "POST", body: fd, credentials: "include" });
      if (!res.ok) throw new Error("生成できませんでした");
      // 担当が決まらない案件があると、その分は作らず、件数と案内が JSON で返る
      if (res.headers.get("content-type")?.includes("application/json")) {
        return (await res.json().catch(() => null)) as { skipped_no_owner?: number; message?: string } | null;
      }
      return null;
    },
    onSuccess: (r) => {
      queryClient.invalidateQueries({ queryKey: ["tasks"] });
      if (r?.skipped_no_owner) toast.warning(`タスクを生成しました。${r.skipped_no_owner}件は作れませんでした。${r.message ?? ""}`);
      else toast.success("タスク生成完了");
    },
    onError: (e: Error) => toast.error(`タスク生成に失敗しました: ${e.message}`),
  });

  const completeMut = useMutation({
    mutationFn: (id: number) => fetch(`/api/v1/tasks/${id}/complete`, { method: "POST", credentials: "include" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "完了に失敗しました"); } return r; }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["tasks"] }),
  });

  const skipMut = useMutation({
    mutationFn: (id: number) => fetch(`/api/v1/tasks/${id}/skip`, { method: "POST", credentials: "include" }).then(async r => { if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "スキップに失敗しました"); } return r; }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["tasks"] }),
  });

  const summary = data?.summary;

  return (
    <div className="p-6 space-y-6">
      <PageHeader
        actions={
          <>
          <input
            type="month"
            value={genMonth}
            onChange={(e) => setGenMonth(e.target.value)}
            className="bg-muted border border-border text-sm text-foreground rounded-md px-2 py-1.5"
          />
          <button
            // タスク画面は撤去予定(UI刷新フェーズ4)のため、confirm のまま残す
            // eslint-disable-next-line no-restricted-globals
            onClick={() => { if (confirm(`${genMonth}のタスクを自動生成しますか？`)) generateMut.mutate(genMonth); }}
            disabled={generateMut.isPending}
            className="px-3 py-1.5 text-xs font-medium rounded-md bg-teal-500/10 text-teal-400 border border-teal-500/30 hover:bg-teal-500/20 transition-colors disabled:opacity-50"
          >
            ⚡ タスク生成
          </button>
          </>
        }
      />

      {/* サマリー */}
      <div className="grid grid-cols-5 gap-3">
        {[
          { label: "合計", value: summary?.total ?? 0, color: "text-foreground" },
          { label: "完了", value: summary?.done ?? 0, color: "text-emerald-400" },
          { label: "未完了", value: summary?.pending ?? 0, color: "text-amber-400" },
          { label: "期限超過", value: summary?.overdue ?? 0, color: "text-red-400" },
          { label: "完了率", value: `${summary?.completion_rate ?? 0}%`, color: "text-blue-400" },
        ].map((s) => (
          <Card key={s.label} className="bg-card border-border">
            <CardContent className="p-3">
              <p className="text-[11px] text-muted-foreground">{s.label}</p>
              <p className={cn("text-2xl font-bold tabular-nums", s.color)}>{s.value}</p>
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
                <TableHead className="text-xs text-muted-foreground">ID</TableHead>
                <TableHead className="text-xs text-muted-foreground">タスク種別</TableHead>
                <TableHead className="text-xs text-muted-foreground">担当</TableHead>
                <TableHead className="text-xs text-muted-foreground">稼働月</TableHead>
                <TableHead className="text-xs text-muted-foreground">期限</TableHead>
                <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
                <TableHead className="text-xs text-muted-foreground">備考</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(data?.tasks ?? []).length === 0 ? (
                <TableRow><TableCell colSpan={8} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : data?.tasks.map((t) => {
                const st = getStatus("task", t.status);
                const today = new Date().toISOString().slice(0, 10);
                const isOverdue = t.status !== "DONE" && t.status !== "SKIPPED" && t.deadline < today;
                const isPending = t.status === "PENDING";
                return (
                  <TableRow key={t.id} className={cn("border-border/50", isOverdue && "bg-red-500/[0.04]")}>
                    <TableCell className="text-sm tabular-nums text-muted-foreground">{t.id}</TableCell>
                    <TableCell className="text-sm font-medium">{t.task_type}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{t.responsible}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{t.work_month?.slice(0, 7)}</TableCell>
                    <TableCell className={cn("text-sm tabular-nums", isOverdue ? "text-red-400" : "text-muted-foreground")}>{t.deadline}</TableCell>
                    <TableCell><Badge variant="outline" className={cn("text-[10px]", st.className)}>{st.label}</Badge></TableCell>
                    <TableCell className="text-[11px] text-muted-foreground max-w-40 truncate">{t.note || "—"}</TableCell>
                    <TableCell className="text-center">
                      {/* 完了(本線)とスキップ(例外)は消さず、対応済みの行では非活性にする(行ごとに位置がずれない) */}
                      <div className="flex items-center justify-center gap-1">
                        <button
                          onClick={(e) => { e.stopPropagation(); completeMut.mutate(t.id); }}
                          disabled={!isPending || completeMut.isPending}
                          title={!isPending ? "対応済みです" : undefined}
                          className="px-2 py-0.5 text-[10px] rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                        >
                          完了
                        </button>
                        <button
                          onClick={(e) => { e.stopPropagation(); skipMut.mutate(t.id); }}
                          disabled={!isPending || skipMut.isPending}
                          title={!isPending ? "対応済みです" : undefined}
                          className="px-2 py-0.5 text-[10px] rounded bg-amber-500/10 text-amber-400 border border-amber-500/30 hover:bg-amber-500/20 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                        >
                          スキップ
                        </button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </div>
    </div>
  );
}
