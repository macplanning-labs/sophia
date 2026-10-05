"use client";

/**
 * ポータル稼働報告ページ
 *
 * 2タブ構成:
 * - ファイルアップロード（D&D → 2ステップ: 解析プレビュー → 確定登録）
 * - 日次入力（既存 timesheet-entry コンポーネント）
 */

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { TimesheetPreview } from "@/components/TimesheetPreview";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { Upload, FileSpreadsheet, Pencil, Send } from "lucide-react";
import { toast } from "sonner";
import { useTimesheetUploadFlow } from "@/hooks/useTimesheetUploadFlow";
import { fetchPortalTimesheets, submitPortalTimesheet } from "@/lib/api";

type TabId = "upload" | "daily";

export default function PortalTimesheetsPage() {
  const [activeTab, setActiveTab] = useState<TabId>("upload");

  const {
    fileInputRef,
    preview,
    excelBuffer,
    pdfUrl,
    dragOver,
    isParsing,
    isConfirming,
    handleDrop,
    handleDragOver,
    handleDragLeave,
    handleFileChange,
    handleConfirm,
    handleCancel,
  } = useTimesheetUploadFlow({
    uploadUrl: "/api/v1/portal/timesheets/upload",
    confirmUrl: "/api/v1/portal/timesheets/confirm",
    invalidateKeys: [["portal-timesheets"]],
    toastOnParseSuccess: true,
    validateFile: true,
  });

  // 稼働報告一覧
  const { data: listData } = useQuery({
    queryKey: ["portal-timesheets"],
    queryFn: fetchPortalTimesheets,
  });

  const timesheets = listData?.timesheets ?? [];

  return (
    <div className="p-6 space-y-4">
      <div>
        <h1 className="text-xl font-bold text-foreground flex items-center gap-2">
          <span className="text-blue-400">📝</span> 稼働報告
        </h1>
        <p className="text-xs text-muted-foreground mt-1">月次の稼働報告書をアップロードまたは日次入力で提出</p>
      </div>

      {/* タブ */}
      <div className="flex gap-1 bg-muted/30 rounded-lg p-1 w-fit">
        <TabButton active={activeTab === "upload"} onClick={() => setActiveTab("upload")} icon={Upload} label="ファイルアップロード" />
        <TabButton active={activeTab === "daily"} onClick={() => setActiveTab("daily")} icon={Pencil} label="日次入力" />
      </div>

      {activeTab === "upload" && (
        <div className="space-y-4">
          {/* プレビュー表示中 or アップロードエリア */}
          {preview && (excelBuffer || pdfUrl) ? (
            <TimesheetPreview
              preview={preview}
              excelBuffer={excelBuffer}
              pdfUrl={pdfUrl}
              onConfirm={handleConfirm}
              onCancel={handleCancel}
              isConfirming={isConfirming}
            />
          ) : (
            <div
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              className={cn(
                "border-2 border-dashed rounded-xl p-12 text-center transition-all cursor-pointer",
                dragOver
                  ? "border-blue-400 bg-blue-500/5 scale-[1.01]"
                  : "border-border hover:border-muted-foreground/50 hover:bg-accent/20",
                isParsing && "opacity-50 pointer-events-none"
              )}
              onClick={() => fileInputRef.current?.click()}
            >
              <input
                ref={fileInputRef}
                type="file"
                accept=".xlsx,.xlsm,.pdf,application/pdf"
                onChange={handleFileChange}
                className="hidden"
              />
              <div className="flex flex-col items-center gap-3">
                <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-blue-500/20 to-cyan-500/20 flex items-center justify-center">
                  <FileSpreadsheet className="w-7 h-7 text-blue-400" />
                </div>
                <div>
                  <p className="text-sm font-medium text-foreground">
                    {isParsing ? "解析中..." : "稼働報告書をドラッグ＆ドロップ"}
                  </p>
                  <p className="text-xs text-muted-foreground mt-1">
                    対応形式: .xlsx / .xlsm / .pdf（50MBまで）
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* アップロード済み一覧 */}
          <div className="bg-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2.5 border-b border-border bg-muted/30">
              <span className="text-xs font-medium text-foreground">アップロード済み稼働報告</span>
            </div>
            <Table>
              <TableHeader>
                <TableRow className="border-border hover:bg-transparent">
                  <TableHead className="text-xs text-muted-foreground">対象月</TableHead>
                  <TableHead className="text-xs text-muted-foreground">エンジニア</TableHead>
                  <TableHead className="text-xs text-muted-foreground text-right">合計時間</TableHead>
                  <TableHead className="text-xs text-muted-foreground">ステータス</TableHead>
                  <TableHead className="text-xs text-muted-foreground w-20" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {timesheets.length === 0 ? (
                  <TableRow><TableCell colSpan={5} className="text-center py-8 text-muted-foreground text-sm">まだ稼働報告がありません</TableCell></TableRow>
                ) : timesheets.map((ts: any) => (
                  <TableRow key={ts.id} className="border-border/50">
                    <TableCell className="text-sm">{ts.target_month}</TableCell>
                    <TableCell className="text-sm">{ts.engineer_name}</TableCell>
                    <TableCell className="text-sm text-right tabular-nums">{ts.total_hours}h</TableCell>
                    <TableCell>
                      <Badge variant="outline" className={cn(
                        "text-[10px]",
                        ts.status === "submitted"
                          ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                          : "bg-amber-500/10 text-amber-400 border-amber-500/30"
                      )}>
                        {ts.status === "submitted" ? "提出済" : "未提出"}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      {ts.status !== "submitted" && (
                        <SubmitButton id={ts.id} />
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>
      )}

      {activeTab === "daily" && (
        <div className="bg-card border border-border rounded-lg p-8 text-center">
          <p className="text-sm text-muted-foreground">
            日次入力機能は準備中です。<br />
            <a href="/portal/timesheet-entry" className="text-blue-400 hover:underline">旧版の日次入力画面</a>をご利用ください。
          </p>
        </div>
      )}
    </div>
  );
}

// ── サブコンポーネント ──

function TabButton({ active, onClick, icon: Icon, label }: { active: boolean; onClick: () => void; icon: React.ElementType; label: string }) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md transition-colors",
        active ? "bg-card text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground"
      )}
    >
      <Icon className="w-3.5 h-3.5" />
      {label}
    </button>
  );
}

function SubmitButton({ id }: { id: number }) {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: () => submitPortalTimesheet(id.toString()),
    onSuccess: () => {
      toast.success("提出しました");
      queryClient.invalidateQueries({ queryKey: ["portal-timesheets"] });
    },
    onError: (err: Error) => toast.error(err.message || "提出に失敗しました"),
  });

  return (
    <button
      onClick={() => mutation.mutate()}
      disabled={mutation.isPending}
      className="flex items-center gap-1 px-2 py-1 text-[10px] font-medium text-blue-400 hover:bg-blue-500/10 rounded transition-colors"
    >
      <Send className="w-3 h-3" />
      提出
    </button>
  );
}
