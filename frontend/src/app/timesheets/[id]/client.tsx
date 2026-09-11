"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { fetchTimesheetDetail } from "@/lib/api";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { GuidanceCallout } from "@/components/ui/guidance-callout";
import { toast } from "sonner";
import { useDynamicId } from "@/lib/utils";
import { getTimesheetStatusGuidance } from "@/lib/guidance/timesheet";

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
}

export default function TimesheetDetailPage({ id: idProp, embedded = false }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: ["timesheets", id],
    queryFn: () => fetchTimesheetDetail(id),
    enabled: !!id,
  });

  const approveMut = useMutation({
    mutationFn: async () => { const r = await fetch(`/api/v1/timesheets/${id}/approve`, { method: "POST" }); if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "承認に失敗しました"); } return r; },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["timesheets", id] });
      queryClient.invalidateQueries({ queryKey: ["timesheets"], exact: true });
      queryClient.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success("承認しました");
    },
  });

  const rejectMut = useMutation({
    mutationFn: async () => { const r = await fetch(`/api/v1/timesheets/${id}/reject`, { method: "POST" }); if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "差戻しに失敗しました"); } return r; },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["timesheets", id] });
      queryClient.invalidateQueries({ queryKey: ["timesheets"], exact: true });
      queryClient.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success("差戻ししました");
    },
  });

  const sendMut = useMutation({
    mutationFn: async () => { const r = await fetch(`/api/v1/timesheets/${id}/send`, { method: "POST" }); if (!r.ok) { const b = await r.json().catch(() => ({})); throw new Error(b.error || "送信に失敗しました"); } return r; },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["timesheets", id] });
      queryClient.invalidateQueries({ queryKey: ["timesheets"], exact: true });
      queryClient.invalidateQueries({ queryKey: ["dashboard"] });
      toast.success("クライアントに送信しました");
    },
  });

  const sheet = data?.sheet;
  const status = sheet?.status;

  const actions = (
    <div className="flex items-center gap-2">
      {(status === "UPLOADED" || status === "PARSED") && (
        <>
          <button
            onClick={() => approveMut.mutate()}
            disabled={approveMut.isPending}
            className="px-3 py-1.5 text-xs font-medium rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20 transition-colors disabled:opacity-50"
          >
            ✅ 承認
          </button>
          <button
            onClick={() => { if (confirm("差戻ししますか？")) rejectMut.mutate(); }}
            disabled={rejectMut.isPending}
            className="px-3 py-1.5 text-xs font-medium rounded-md bg-red-500/10 text-red-400 border border-red-500/30 hover:bg-red-500/20 transition-colors disabled:opacity-50"
          >
            ↩ 差戻し
          </button>
        </>
      )}
      {status === "APPROVED" && (
        <button
          onClick={() => sendMut.mutate()}
          disabled={sendMut.isPending}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-blue-500/10 text-blue-400 border border-blue-500/30 hover:bg-blue-500/20 transition-colors disabled:opacity-50"
        >
          📧 クライアント送信
        </button>
      )}
    </div>
  );

  const guidance = getTimesheetStatusGuidance(status || "");

  return (
    <DetailLayout title={`稼働報告 #${id}`} icon="⏱" backHref="/timesheets" backLabel="一覧に戻る" isLoading={isLoading} actions={actions} embedded={embedded}>
      {sheet && (
        <>
          {guidance && (
            <GuidanceCallout variant="info" {...guidance} />
          )}
          <FieldGrid>
            <Field label="対象月" value={sheet.target_month} />
            <Field label="契約情報" value={data.contract_info} />
            <Field label="ステータス" value={
              <Badge className="text-[10px]">{data.status_display}</Badge>
            } />
            <Field label="時間判定" value={
              <Badge className={`text-[10px] ${
                data.hours_check === "範囲内" ? "bg-emerald-500/20 text-emerald-400 border-emerald-500/30" :
                "bg-red-500/20 text-red-400 border-red-500/30"
              }`}>{data.hours_check}</Badge>
            } />
          </FieldGrid>
          <FieldGrid>
            <Field label="合計時間" value={`${sheet.total_hours}h`} />
            <Field label="稼働日数" value={`${sheet.work_days ?? 0}日`} />
            <Field label="ファイル名" value={sheet.original_filename || "—"} />
            <Field label="更新日" value={sheet.updated_at?.slice(0, 10)} />
          </FieldGrid>

          {/* 関連メール */}
          {(data.source_emails ?? []).length > 0 && (
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <div className="px-4 py-2 border-b border-border">
                <span className="text-sm font-medium text-foreground">📧 関連メール</span>
              </div>
              <Table>
                <TableHeader>
                  <TableRow className="border-border hover:bg-transparent">
                    <TableHead className="text-xs text-muted-foreground">送信者</TableHead>
                    <TableHead className="text-xs text-muted-foreground">件名</TableHead>
                    <TableHead className="text-xs text-muted-foreground">受信日時</TableHead>
                    <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {(data.source_emails ?? []).map((email: Record<string, unknown>, i: number) => (
                    <TableRow key={i} className="border-border/50">
                      <TableCell className="text-sm">{String(email.from)}</TableCell>
                      <TableCell className="text-sm text-foreground">{String(email.subject)}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{String(email.received_at)}</TableCell>
                      <TableCell><Badge className="text-[10px]">{String(email.status)}</Badge></TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </>
      )}
    </DetailLayout>
  );
}
