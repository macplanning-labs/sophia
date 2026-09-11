"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getStatus, getAllStatuses } from "@/lib/status";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchTimesheets, fetchEngineerOptions } from "@/lib/api";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { GuidanceCallout } from "@/components/ui/guidance-callout";
import { cn } from "@/lib/utils";
import { FileSpreadsheet, AlertTriangle, ChevronDown, ChevronUp, Mail } from "lucide-react";
import { toast } from "sonner";
import { TimesheetPreview } from "@/components/TimesheetPreview";
import { useTimesheetUploadFlow } from "@/hooks/useTimesheetUploadFlow";
import { ActionFilterChips } from "@/components/ui/action-filter-chips";
import {
  applyNeedsActionFilter,
  isTimesheetNeedsAction,
  type ListFilterMode,
} from "@/lib/needs-action";
import { getTimesheetUploadPanelGuidance } from "@/lib/guidance/timesheet";
import TimesheetDetailPage from "./[id]/client";


export default function TimesheetsPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <TimesheetsPageContent />
    </Suspense>
  );
}

function TimesheetsPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [editId, setEditId] = useState<string | null>(null);
  const [showUpload, setShowUpload] = useState(false);
  const [filterMode, setFilterMode] = useState<ListFilterMode>("pending");
  const [engineerFilter, setEngineerFilter] = useState("");
  const [monthFilter, setMonthFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");

  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setEditId(fromUrl);
  }, [searchParams]);

  useEffect(() => {
    if (searchParams.get("upload") === "1") setShowUpload(true);
  }, [searchParams]);

  const openEdit = useCallback((id: string | number) => {
    const idStr = String(id);
    setEditId(idStr);
    router.replace(`/timesheets?edit=${encodeURIComponent(idStr)}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/timesheets", { scroll: false });
  }, [router]);

  // 招待モーダル用
  const [showInvite, setShowInvite] = useState(false);
  const [inviteEngineerId, setInviteEngineerId] = useState<number | null>(null);
  const [inviteEmail, setInviteEmail] = useState("");
  const [isInviting, setIsInviting] = useState(false);

  const {
    fileInputRef,
    preview,
    excelBuffer,
    pdfUrl,
    dragOver,
    uploadError,
    isParsing,
    isConfirming,
    processFile,
    handleDrop,
    handleDragOver,
    handleDragLeave,
    handleConfirm,
    handleCancel,
    clearPreviewState,
  } = useTimesheetUploadFlow({
    uploadUrl: "/api/v1/timesheets/upload",
    confirmUrl: "/api/v1/timesheets/confirm",
    invalidateKeys: [["timesheets"]],
    trackUploadError: true,
    onConfirmSuccess: () => setShowUpload(false),
  });

  const { data, isLoading } = useQuery({
    queryKey: ["timesheets"],
    queryFn: fetchTimesheets,
  });

  const allTimesheets = data?.timesheets ?? [];
  const pendingCount = useMemo(
    () => allTimesheets.filter((t) => isTimesheetNeedsAction(t.status)).length,
    [allTimesheets]
  );

  const engineerOptions = useMemo(() => {
    const names = new Set<string>();
    for (const t of allTimesheets) {
      if (t.engineer_name) names.add(t.engineer_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [allTimesheets]);

  const monthOptions = useMemo(() => {
    const months = new Set<string>();
    for (const t of allTimesheets) {
      const ym = t.target_month?.slice(0, 7);
      if (ym) months.add(ym);
    }
    return [...months]
      .sort((a, b) => b.localeCompare(a))
      .map((ym) => {
        const [y, m] = ym.split("-");
        return { value: ym, label: `${y}年${Number(m)}月` };
      });
  }, [allTimesheets]);

  const statusOptions = useMemo(() => {
    const present = new Set(allTimesheets.map((t) => t.status));
    return getAllStatuses("timesheet")
      .filter((s) => present.has(s.value))
      .map((s) => ({ value: s.value, label: s.label }));
  }, [allTimesheets]);

  const filteredTimesheets = useMemo(() => {
    const byAction = applyNeedsActionFilter(allTimesheets, filterMode, (t) => isTimesheetNeedsAction(t.status));
    return byAction.filter((t) => {
      if (engineerFilter && t.engineer_name !== engineerFilter) return false;
      if (monthFilter && t.target_month?.slice(0, 7) !== monthFilter) return false;
      if (statusFilter && t.status !== statusFilter) return false;
      return true;
    });
  }, [allTimesheets, filterMode, engineerFilter, monthFilter, statusFilter]);

  const { isAdmin } = useCurrentUser();

  // 全エンジニア取得（招待用）
  // ※ 招待の実行自体(POST /api/v1/invite)はサーバー側でAdmin専用のため、
  //   ボタン自体もAdminのみ表示する（一般社員には非表示 = enabled: isAdmin && showInvite）
  const { data: engineersRaw } = useQuery({
    queryKey: ["engineer-options"],
    queryFn: fetchEngineerOptions,
    enabled: isAdmin && showInvite,
  });
  const engineers = engineersRaw ?? [];

  // 招待送信
  const handleInviteAdmin = async () => {
    if (!inviteEngineerId || !inviteEmail) {
      toast.error("エンジニアとメールアドレスを選択・入力してください");
      return;
    }
    setIsInviting(true);
    try {
      const res = await fetch("/api/v1/invite", {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ engineer_id: inviteEngineerId, email: inviteEmail }),
      });
      const data = await res.json();
      if (res.ok && data.success) {
        toast.success(data.message);
        setShowInvite(false);
        setInviteEmail("");
      } else {
        toast.error(data.error || "招待に失敗しました");
      }
    } catch {
      toast.error("通信エラーが発生しました");
    } finally {
      setIsInviting(false);
    }
  };

  const summary = data?.summary;

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <div className="flex items-center gap-2">
          {isAdmin && (
            <button
              onClick={() => setShowInvite(true)}
              className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border border-primary/30 bg-primary/10 text-primary hover:bg-primary/20 transition-colors"
            >
              <Mail className="w-3.5 h-3.5" />
              エンジニアを招待
            </button>
          )}
          <button
            onClick={() => {
              setShowUpload(!showUpload);
              if (!showUpload) clearPreviewState();
            }}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-md border transition-colors",
              showUpload
                ? "bg-muted/60 text-foreground border-border"
                : "bg-blue-500/10 text-blue-400 border-blue-500/30 hover:bg-blue-500/20"
            )}
          >
            <FileSpreadsheet className="w-3.5 h-3.5" />
            アップロード
            {showUpload ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
          </button>
        </div>
      </div>

      {/* 招待モーダル */}
      {showInvite && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <div className="bg-card border border-border rounded-xl shadow-2xl w-full max-w-md overflow-hidden">
            <div className="px-6 py-4 border-b border-border flex items-center justify-between">
              <h2 className="text-lg font-bold flex items-center gap-2">
                <Mail className="w-5 h-5 text-primary" />
                エンジニア招待
              </h2>
              <button onClick={() => setShowInvite(false)} className="text-muted-foreground hover:text-foreground">✕</button>
            </div>
            <div className="p-6 space-y-4">
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground">対象エンジニア</label>
                <select
                  className="w-full bg-muted border border-border rounded-lg px-3 py-2 text-sm text-foreground focus:ring-2 focus:ring-primary/20 outline-none"
                  value={inviteEngineerId ?? ""}
                  onChange={(e) => {
                    const id = Number(e.target.value);
                    setInviteEngineerId(id);
                    const eng = engineers.find((en: any) => en.id === id);
                    setInviteEmail(eng?.email ? String(eng.email) : "");
                  }}
                >
                  <option value="">選択してください...</option>
                  {engineers.map((eng: any) => (
                    <option key={eng.id} value={eng.id}>{eng.name}</option>
                  ))}
                </select>
              </div>
              <div className="space-y-1.5">
                <label className="text-xs font-medium text-muted-foreground">招待先メールアドレス</label>
                {(() => {
                  const selectedEng = engineers.find((en: any) => en.id === inviteEngineerId);
                  const hasMasterEmail = selectedEng?.email;
                  return (
                    <>
                      <input
                        type="email"
                        placeholder={inviteEngineerId ? "メールアドレスを入力..." : "先にエンジニアを選択してください"}
                        className={cn(
                          "w-full border rounded-lg px-3 py-2 text-sm focus:ring-2 focus:ring-primary/20 outline-none",
                          hasMasterEmail
                            ? "bg-emerald-500/10 border-emerald-500/30 text-foreground"
                            : "bg-muted border-border text-foreground"
                        )}
                        value={inviteEmail}
                        onChange={(e) => setInviteEmail(e.target.value)}
                        readOnly={!!hasMasterEmail}
                        disabled={!inviteEngineerId}
                      />
                      {inviteEngineerId && !hasMasterEmail && (
                        <p className="text-[11px] text-amber-400">⚠ 技術者マスタにメールアドレスが未登録です。手動で入力してください。</p>
                      )}
                      {hasMasterEmail && (
                        <p className="text-[11px] text-emerald-400">✓ 技術者マスタから取得</p>
                      )}
                    </>
                  );
                })()}
              </div>
              <p className="text-[11px] text-muted-foreground bg-muted/50 p-3 rounded-lg">
                ※ 招待メールにはログインURLが含まれます。エンジニアはリンクをクリックするだけでログインし、稼働報告を開始できます。パスワードは不要です。
              </p>
            </div>
            <div className="px-6 py-4 border-t border-border bg-muted/20 flex justify-end gap-3">
              <button
                onClick={() => setShowInvite(false)}
                className="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
              >
                キャンセル
              </button>
              <button
                onClick={handleInviteAdmin}
                disabled={isInviting || !inviteEngineerId || !inviteEmail}
                className="bg-primary text-primary-foreground px-6 py-2 rounded-lg text-sm font-bold shadow-lg shadow-primary/20 hover:bg-primary/90 transition-all active:scale-95 disabled:opacity-50"
              >
                {isInviting ? "送信中..." : "招待メールを送信"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* サマリー */}
      <div className="grid grid-cols-4 gap-3">
        {[
          { label: "合計", value: summary?.total ?? 0, color: "text-foreground" },
          { label: "未提出", value: summary?.pending ?? 0, color: "text-amber-400" },
          { label: "提出済", value: summary?.uploaded ?? 0, color: "text-blue-400" },
          { label: "承認済", value: summary?.approved ?? 0, color: "text-emerald-400" },
        ].map((s) => (
          <Card key={s.label} className="bg-card border-border">
            <CardContent className="p-3">
              <p className="text-[11px] text-muted-foreground">{s.label}</p>
              <p className={cn("text-2xl font-bold tabular-nums", s.color)}>{s.value}</p>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* インラインアップロードエリア */}
      {showUpload && (
        <div className="bg-card border border-border rounded-lg p-4 space-y-3">
          <GuidanceCallout variant="info" {...getTimesheetUploadPanelGuidance()} />
          <div
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
            onClick={() => fileInputRef.current?.click()}
            className={cn(
              "border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-all duration-200",
              dragOver ? "border-blue-400 bg-blue-500/10" : "border-border hover:border-border hover:bg-muted/50"
            )}
          >
            <FileSpreadsheet className="w-8 h-8 mx-auto mb-2 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">Excel（.xlsx / .xlsm）または勤務表PDFをドラッグ＆ドロップ</p>
            <p className="text-xs text-muted-foreground mt-1">またはクリックしてファイルを選択</p>
            <input
              ref={fileInputRef}
              type="file"
              accept=".xlsx,.xlsm,.pdf,application/pdf"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) processFile(f);
              }}
            />
          </div>

          {isParsing && (
            <div className="text-center py-2">
              <div className="w-5 h-5 border-2 border-blue-400 border-t-transparent rounded-full animate-spin mx-auto" />
              <p className="text-xs text-muted-foreground mt-2">解析中...</p>
            </div>
          )}

          {uploadError && (
            <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 flex items-start gap-2">
              <AlertTriangle className="w-4 h-4 text-red-400 mt-0.5 shrink-0" />
              <p className="text-sm text-red-400">{uploadError}</p>
            </div>
          )}

          {preview && (excelBuffer || pdfUrl) && (
            <TimesheetPreview
              preview={preview}
              excelBuffer={excelBuffer}
              pdfUrl={pdfUrl}
              onConfirm={handleConfirm}
              onCancel={() => {
                handleCancel();
              }}
              isConfirming={isConfirming}
            />
          )}
        </div>
      )}

      {/* テーブル */}
      <ActionFilterChips
        value={filterMode}
        onChange={setFilterMode}
        pendingCount={pendingCount}
        totalCount={allTimesheets.length}
      />
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">ID</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="作業者"
                    options={engineerOptions}
                    value={engineerFilter}
                    onChange={setEngineerFilter}
                    placeholder="作業者名で検索…"
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
                <TableHead className="text-xs text-muted-foreground">契約情報</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="ステータス"
                    options={statusOptions}
                    value={statusFilter}
                    onChange={setStatusFilter}
                    placeholder="ステータスで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">稼働時間</TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">稼働日数</TableHead>
                <TableHead className="text-xs text-muted-foreground">ファイル</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredTimesheets.length === 0 ? (
                <TableRow><TableCell colSpan={8} className="text-center py-12 text-muted-foreground">
                  {allTimesheets.length === 0 ? "データがありません" : "条件に一致するデータがありません"}
                </TableCell></TableRow>
              ) : filteredTimesheets.map((t) => {
                const st = getStatus("timesheet", t.status);
                return (
                  <TableRow key={t.id} className="border-border/50 cursor-pointer hover:bg-accent/50" onClick={() => openEdit(t.id)}>
                    <TableCell className="text-sm tabular-nums text-muted-foreground">{t.id}</TableCell>
                    <TableCell className="text-sm font-medium">{t.engineer_name}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{t.target_month?.slice(0, 7)}</TableCell>
                    <TableCell className="text-[11px] text-muted-foreground">{t.contract_info}</TableCell>
                    <TableCell><Badge variant="outline" className={cn("text-[10px]", st.className)}>{st.label}</Badge></TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{t.total_hours}h</TableCell>
                    <TableCell className="text-right text-sm tabular-nums">{t.work_days}日</TableCell>
                    <TableCell className="text-[11px] text-muted-foreground max-w-32 truncate">{t.original_filename || "—"}</TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </div>

      {editId && (
        <DetailModal open title={`稼働報告 #${editId}`} icon="⏱" size="xl" onClose={closeEdit}>
          <TimesheetDetailPage id={editId} embedded />
        </DetailModal>
      )}
    </div>
  );
}
