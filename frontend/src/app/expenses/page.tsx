"use client";

import { Suspense, useCallback, useEffect, useMemo, useRef, useState, type ClipboardEvent, type DragEvent } from "react";
import { getStatus } from "@/lib/status";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import {
  fetchExpenses, fetchExpenseDetail, createExpense, updateExpense, updateExpenseItem, addExpenseItem,
  deleteExpenseItem, deleteExpense, fetchEmployeeOptions, fetchExpenseCategoryOptions,
  issueMobileUploadToken, fetchMobileUploadStatus, expenseItemReceiptUrl,
  uploadExpenseItemReceipt,
} from "@/lib/api";
import type { ExpenseItem, ExpenseItemForm } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import ExpenseDetailPage from "./[id]/client";
import QRCode from "qrcode";

const EXPENSE_STATUS_OPTIONS = [
  { value: "DRAFT", label: "下書き" },
  { value: "PENDING", label: "申請中" },
  { value: "APPROVED", label: "承認済" },
  { value: "REJECTED", label: "却下" },
];

// 明細1行（サーバー側にまだ存在しない行は id が null）
type LocalItem = {
  localKey: string;
  id: number | null;
  expense_date: string;
  category: string;
  description: string;
  amount: number;
  has_receipt: boolean;
  receipt_mime: string | null;
};

function emptyItem(): LocalItem {
  return {
    localKey: crypto.randomUUID(),
    id: null,
    expense_date: new Date().toISOString().slice(0, 10),
    category: "",
    description: "",
    amount: 0,
    has_receipt: false,
    receipt_mime: null,
  };
}

function isReceiptFile(file: File) {
  const t = file.type.toLowerCase();
  if (t === "image/jpeg" || t === "image/jpg" || t === "image/png" || t === "application/pdf") {
    return true;
  }
  // クリップボード等で type が空のとき拡張子で判定
  return /\.(jpe?g|png|pdf)$/i.test(file.name);
}

function ReceiptThumb({ itemId, mime }: { itemId: number; mime: string | null }) {
  const url = `${expenseItemReceiptUrl(itemId)}?t=${Date.now()}`;
  if (mime === "application/pdf") {
    return (
      <span className="inline-flex h-8 w-8 items-center justify-center rounded border border-border bg-muted text-[10px] font-semibold text-rose-300">
        PDF
      </span>
    );
  }
  return <img src={url} alt="領収書" className="h-8 w-8 object-cover rounded border border-border" />;
}

export default function ExpensesPage() {
  return (
    <Suspense fallback={<div className="p-8 text-sm text-muted-foreground">読み込み中...</div>}>
      <ExpensesPageContent />
    </Suspense>
  );
}

function ExpensesPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const queryClient = useQueryClient();
  const [modalMode, setModalMode] = useState<"create" | "edit" | null>(null);
  const [editHeaderId, setEditHeaderId] = useState<number | null>(null);
  const [employeeId, setEmployeeId] = useState<number>(0);
  const [items, setItems] = useState<LocalItem[]>([]);
  const [deleteTarget, setDeleteTarget] = useState<{ id: number; label: string } | null>(null);
  const [detailEditId, setDetailEditId] = useState<string | null>(null);
  const [qrItem, setQrItem] = useState<LocalItem | null>(null);
  const [dragOverKey, setDragOverKey] = useState<string | null>(null);
  const [nameFilter, setNameFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [monthFilter, setMonthFilter] = useState("");

  const { data: expenses, isLoading } = useQuery({
    queryKey: ["expenses"],
    queryFn: fetchExpenses,
  });

  const expenseRows = expenses ?? [];

  const nameOptions = useMemo(() => {
    const names = new Set<string>();
    for (const ex of expenseRows) {
      if (ex.employee_name) names.add(ex.employee_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [expenseRows]);

  const statusOptions = useMemo(() => {
    const present = new Set(expenseRows.map((ex) => ex.status));
    return EXPENSE_STATUS_OPTIONS.filter((o) => present.has(o.value));
  }, [expenseRows]);

  const monthOptions = useMemo(() => {
    const months = new Set<string>();
    for (const ex of expenseRows) {
      const ym = ex.created_at?.slice(0, 7);
      if (ym) months.add(ym);
    }
    return [...months]
      .sort((a, b) => b.localeCompare(a))
      .map((ym) => {
        const [y, m] = ym.split("-");
        return { value: ym, label: `${y}年${Number(m)}月` };
      });
  }, [expenseRows]);

  const filteredExpenses = useMemo(
    () =>
      expenseRows.filter((ex) => {
        if (nameFilter && ex.employee_name !== nameFilter) return false;
        if (statusFilter && ex.status !== statusFilter) return false;
        if (monthFilter && ex.created_at?.slice(0, 7) !== monthFilter) return false;
        return true;
      }),
    [expenseRows, nameFilter, statusFilter, monthFilter]
  );

  const { data: categories = [] } = useQuery({
    queryKey: ["expense-categories"],
    queryFn: fetchExpenseCategoryOptions,
  });

  const { data: employees } = useQuery({
    queryKey: ["employee-options"],
    queryFn: fetchEmployeeOptions,
  });

  const deleteMutation = useMutation({
    mutationFn: deleteExpense,
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ["expenses"] }); setDeleteTarget(null); },
  });

  const openCreate = () => {
    setModalMode("create");
    setEditHeaderId(null);
    setEmployeeId(0);
    setItems([emptyItem()]);
  };

  const openEdit = async (headerId: number) => {
    const detail = await fetchExpenseDetail(String(headerId));
    setModalMode("edit");
    setEditHeaderId(headerId);
    setEmployeeId(detail.expense.employee_id);
    setItems((detail.items.length > 0 ? detail.items : []).map((it: ExpenseItem) => ({
      localKey: crypto.randomUUID(),
      id: it.id,
      expense_date: it.expense_date,
      category: it.category,
      description: it.description,
      amount: it.amount,
      has_receipt: it.has_receipt,
      receipt_mime: it.receipt_mime ?? null,
    })));
  };

  // 詳細画面の「編集」から ?form={id} で戻ってきたとき、編集モーダルを開く
  useEffect(() => {
    const formId = Number(searchParams.get("form") || 0);
    if (!formId) return;
    let cancelled = false;
    void openEdit(formId).then(() => {
      if (!cancelled) router.replace("/expenses", { scroll: false });
    });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- form クエリ変化時のみ
  }, [searchParams, router]);

  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setDetailEditId(fromUrl);
  }, [searchParams]);

  const openDetail = useCallback((id: string | number) => {
    const idStr = String(id);
    setDetailEditId(idStr);
    router.replace(`/expenses?edit=${encodeURIComponent(idStr)}`, { scroll: false });
  }, [router]);

  const closeDetail = useCallback(() => {
    setDetailEditId(null);
    router.replace("/expenses", { scroll: false });
  }, [router]);

  const openFormFromDetail = useCallback((id: string) => {
    closeDetail();
    void openEdit(Number(id));
  }, [closeDetail, openEdit]);

  const handleRowClick = (ex: { id: number }) => {
    openDetail(ex.id);
  };

  const closeModal = () => {
    setModalMode(null);
    setEditHeaderId(null);
    setItems([]);
  };

  const itemAsForm = (item: LocalItem): ExpenseItemForm => ({
    expense_date: item.expense_date,
    category: item.category,
    description: item.description,
    amount: item.amount,
  });

  /** ヘッダー・明細の未保存分を保存し、保存後のヘッダーIDと明細一覧を返す */
  const ensureSaved = async (): Promise<{ headerId: number; items: LocalItem[] } | null> => {
    if (employeeId === 0) {
      toast.error("申請者を選択してください");
      return null;
    }
    if (items.some((it) => !it.category || it.amount <= 0)) {
      toast.error("すべての明細でカテゴリと金額（0円超）を入力してください");
      return null;
    }

    let headerId = editHeaderId;

    if (headerId === null) {
      const res = await createExpense({ employee_id: employeeId, items: items.map(itemAsForm) });
      if (!res.success || !res.id) {
        toast.error(res.error || "保存に失敗しました");
        return null;
      }
      headerId = res.id;
      const newItemIds = res.item_ids ?? [];
      const resolvedItems = items.map((it, i) => ({ ...it, id: newItemIds[i] ?? it.id }));
      setItems(resolvedItems);
      setEditHeaderId(headerId);
      return { headerId, items: resolvedItems };
    }

    // 既存ヘッダー: 申請者（社員）を更新してから明細を同期
    const headerRes = await updateExpense(headerId, { employee_id: employeeId });
    if (!headerRes.success) {
      toast.error(headerRes.error || "申請者の更新に失敗しました");
      return null;
    }

    let resolvedItems = [...items];
    for (const it of resolvedItems) {
      if (it.id === null) {
        const res = await addExpenseItem(headerId, itemAsForm(it));
        if (!res.success || !res.id) {
          toast.error(res.error || "明細の追加に失敗しました");
          return null;
        }
        resolvedItems = resolvedItems.map((p) =>
          p.localKey === it.localKey ? { ...p, id: res.id! } : p,
        );
        setItems(resolvedItems);
      } else {
        const res = await updateExpenseItem(it.id, itemAsForm(it));
        if (!res.success) {
          toast.error(res.error || "明細の更新に失敗しました");
          return null;
        }
      }
    }
    return { headerId, items: resolvedItems };
  };

  const saveMutation = useMutation({
    mutationFn: ensureSaved,
    onSuccess: (result) => {
      if (result === null) return;
      queryClient.invalidateQueries({ queryKey: ["expenses"] });
      queryClient.invalidateQueries({ queryKey: ["expenses", String(result.headerId)] });
      toast.success("保存しました");
      closeModal();
    },
  });

  const addRow = () => setItems((prev) => [...prev, emptyItem()]);

  const removeRow = async (item: LocalItem) => {
    if (item.id !== null) {
      const res = await deleteExpenseItem(item.id);
      if (!res.success) {
        toast.error(res.error || "削除に失敗しました");
        return;
      }
    }
    setItems((prev) => prev.filter((it) => it.localKey !== item.localKey));
  };

  const updateRow = (localKey: string, patch: Partial<LocalItem>) => {
    setItems((prev) => prev.map((it) => (it.localKey === localKey ? { ...it, ...patch } : it)));
  };

  /** 未保存の明細なら先に保存し、サーバー側の item id を返す */
  const resolveItemId = async (item: LocalItem): Promise<number | null> => {
    if (item.id !== null) return item.id;
    const result = await ensureSaved();
    if (result === null) return null;
    const targetId = result.items.find((it) => it.localKey === item.localKey)?.id ?? null;
    if (targetId === null) {
      toast.error("保存直後のため少し待ってから再度お試しください");
    }
    return targetId;
  };

  /** 「スマホからアップロード」: 未保存なら先に保存してからトークン発行 */
  const startMobileUpload = async (item: LocalItem) => {
    const targetId = await resolveItemId(item);
    if (targetId === null) return;
    setQrItem({ ...item, id: targetId });
  };

  /** PCからの領収書添付（ファイル選択 / クリップボード貼り付け / DnD） */
  const attachReceiptFile = async (item: LocalItem, file: File) => {
    if (!isReceiptFile(file)) {
      toast.error("JPEG・PNGまたはPDFファイルを指定してください");
      return;
    }
    if (file.size > 10 * 1024 * 1024) {
      toast.error("ファイルサイズが大きすぎます（上限10MB）");
      return;
    }
    const targetId = await resolveItemId(item);
    if (targetId === null) return;

    const res = await uploadExpenseItemReceipt(targetId, file);
    if (!res.success) {
      toast.error(res.error || "領収書のアップロードに失敗しました");
      return;
    }
    const mime =
      file.type === "application/pdf" || /\.pdf$/i.test(file.name)
        ? "application/pdf"
        : file.type === "image/png" || /\.png$/i.test(file.name)
          ? "image/png"
          : "image/jpeg";
    setItems((prev) =>
      prev.map((it) =>
        it.localKey === item.localKey
          ? { ...it, id: targetId, has_receipt: true, receipt_mime: mime }
          : it,
      ),
    );
    toast.success("領収書を添付しました");
  };

  const handleReceiptPaste = async (item: LocalItem, e: ClipboardEvent) => {
    const clipboardItems = e.clipboardData?.items;
    if (!clipboardItems) return;
    for (const clip of Array.from(clipboardItems)) {
      const t = clip.type.toLowerCase();
      if (!(t.startsWith("image/") || t === "application/pdf")) continue;
      const file = clip.getAsFile();
      if (!file) continue;
      e.preventDefault();
      e.stopPropagation();
      await attachReceiptFile(item, file);
      return;
    }
  };

  const handleReceiptDragOver = (item: LocalItem, e: DragEvent) => {
    if (![...e.dataTransfer.types].includes("Files")) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = "copy";
    setDragOverKey(item.localKey);
  };

  const handleReceiptDragLeave = (item: LocalItem, e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const next = e.relatedTarget as Node | null;
    if (next && e.currentTarget.contains(next)) return;
    setDragOverKey((prev) => (prev === item.localKey ? null : prev));
  };

  const handleReceiptDrop = async (item: LocalItem, e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setDragOverKey(null);
    const file = e.dataTransfer.files?.[0];
    if (!file) {
      toast.error("ファイルをドロップしてください");
      return;
    }
    await attachReceiptFile(item, file);
  };

  const totalAmount = items.reduce((sum, it) => sum + (Number(it.amount) || 0), 0);

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <button
          onClick={openCreate}
          className="px-4 py-2 bg-rose-600 hover:bg-rose-500 text-foreground text-sm font-medium rounded-lg transition-colors"
        >
          + 新規申請
        </button>
      </div>
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
                    label="社員名"
                    options={nameOptions}
                    value={nameFilter}
                    onChange={setNameFilter}
                    placeholder="社員名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">明細</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="ステータス"
                    options={statusOptions}
                    value={statusFilter}
                    onChange={setStatusFilter}
                    placeholder="ステータスで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground text-right">合計金額</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="申請日"
                    options={monthOptions}
                    value={monthFilter}
                    onChange={setMonthFilter}
                    placeholder="申請月で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground w-10"></TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {expenseRows.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredExpenses.length === 0 ? (
                <TableRow><TableCell colSpan={7} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredExpenses.map((ex) => {
                const st = getStatus("expense", ex.status);
                return (
                  <TableRow key={ex.id} className="border-border/50 cursor-pointer hover:bg-accent/50" onClick={() => handleRowClick(ex)}>
                    <TableCell className="text-sm tabular-nums text-muted-foreground">{ex.id}</TableCell>
                    <TableCell className="text-sm font-medium">{ex.employee_name}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{ex.item_count}件</TableCell>
                    <TableCell><Badge variant="outline" className={cn("text-[10px]", st.className)}>{st.label}</Badge></TableCell>
                    <TableCell className="text-right text-sm tabular-nums font-medium">¥{ex.total_amount.toLocaleString()}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{ex.created_at?.slice(0, 10)}</TableCell>
                    <TableCell>
                      {ex.status === "PENDING" && (
                        <button
                          onClick={(e) => { e.stopPropagation(); setDeleteTarget({ id: ex.id, label: `#${ex.id}（${ex.employee_name}）` }); }}
                          className="text-muted-foreground hover:text-red-400 transition-colors text-sm"
                          title="削除"
                        >🗑</button>
                      )}
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredExpenses.length}件</span>
        </div>
      </div>

      {/* 新規・編集モーダル */}
      {modalMode && (
        <div className="fixed inset-0 z-50 flex items-start justify-center pt-[6vh] bg-black/60 backdrop-blur-sm overflow-y-auto">
          <div className="bg-card border border-border rounded-xl w-full max-w-3xl mx-4 mb-10 p-6 space-y-5">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-bold text-foreground">{modalMode === "edit" ? "経費申請 編集" : "経費申請"}</h2>
              <button onClick={closeModal} className="text-muted-foreground hover:text-foreground text-xl">✕</button>
            </div>

            <div>
              <label className="block text-xs text-muted-foreground mb-1">申請者</label>
              <select
                className="w-full bg-muted border border-border rounded-lg px-3 py-2 text-sm text-foreground"
                value={employeeId}
                onChange={(e) => setEmployeeId(Number(e.target.value))}
              >
                <option value={0}>選択してください</option>
                {employees?.map((emp) => (
                  <option key={emp.id} value={emp.id}>{emp.display_name}</option>
                ))}
              </select>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold text-foreground">明細</h3>
                <button
                  type="button"
                  onClick={addRow}
                  className="text-xs px-3 py-1.5 border border-border rounded-lg text-muted-foreground hover:text-foreground hover:border-foreground/30 transition-colors"
                >
                  + 行を追加
                </button>
              </div>

              {items.map((item) => (
                <div
                  key={item.localKey}
                  className={cn(
                    "space-y-2 rounded-lg border p-3 transition-colors",
                    dragOverKey === item.localKey
                      ? "border-rose-500 bg-rose-500/10"
                      : "border-border bg-background/40",
                  )}
                  onPaste={(e) => { void handleReceiptPaste(item, e); }}
                  onDragEnter={(e) => handleReceiptDragOver(item, e)}
                  onDragOver={(e) => handleReceiptDragOver(item, e)}
                  onDragLeave={(e) => handleReceiptDragLeave(item, e)}
                  onDrop={(e) => { void handleReceiptDrop(item, e); }}
                >
                  <div className="grid grid-cols-12 gap-2 items-start">
                    <div className="col-span-3">
                      <label className="block text-[11px] text-muted-foreground mb-1">日付</label>
                      <input
                        type="date"
                        className="w-full bg-muted border border-border rounded-lg px-2 py-1.5 text-sm text-foreground"
                        value={item.expense_date}
                        onChange={(e) => updateRow(item.localKey, { expense_date: e.target.value })}
                      />
                    </div>
                    <div className="col-span-3">
                      <label className="block text-[11px] text-muted-foreground mb-1">カテゴリ</label>
                      <select
                        className="w-full bg-muted border border-border rounded-lg px-2 py-1.5 text-sm text-foreground"
                        value={item.category}
                        onChange={(e) => updateRow(item.localKey, { category: e.target.value })}
                      >
                        <option value="">選択</option>
                        {categories.map((c) => (
                          <option key={c.code} value={c.code}>{c.name}</option>
                        ))}
                      </select>
                    </div>
                    <div className="col-span-4">
                      <label className="block text-[11px] text-muted-foreground mb-1">内容（経路等）</label>
                      <input
                        type="text"
                        className="w-full bg-muted border border-border rounded-lg px-2 py-1.5 text-sm text-foreground"
                        value={item.description}
                        onChange={(e) => updateRow(item.localKey, { description: e.target.value })}
                        placeholder="例: 東京→大阪"
                      />
                    </div>
                    <div className="col-span-2">
                      <label className="block text-[11px] text-muted-foreground mb-1">金額</label>
                      <input
                        type="number"
                        className="w-full bg-muted border border-border rounded-lg px-2 py-1.5 text-sm text-foreground"
                        value={item.amount || ""}
                        onChange={(e) => updateRow(item.localKey, { amount: Number(e.target.value) })}
                        placeholder="0"
                      />
                    </div>
                  </div>
                  <div className="flex items-center justify-between gap-2 pt-1">
                    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                      {item.has_receipt && item.id !== null ? (
                        <a href={expenseItemReceiptUrl(item.id)} target="_blank" rel="noreferrer" className="flex items-center gap-1.5 text-xs text-emerald-400">
                          <ReceiptThumb itemId={item.id} mime={item.receipt_mime} />
                          領収書あり
                        </a>
                      ) : (
                        <span className="text-xs text-muted-foreground">
                          {dragOverKey === item.localKey ? "ここにドロップ" : "領収書未添付"}
                        </span>
                      )}
                      <label
                        tabIndex={0}
                        onPaste={(e) => { void handleReceiptPaste(item, e); }}
                        className={cn(
                          "inline-flex cursor-pointer items-center gap-1.5 rounded-lg border border-dashed px-2.5 py-1 text-xs transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-rose-500",
                          dragOverKey === item.localKey
                            ? "border-rose-400 text-rose-300"
                            : "border-border text-muted-foreground hover:border-foreground/30 hover:text-foreground",
                        )}
                        title="クリックで選択 / ドラッグ&ドロップ / Ctrl・Cmd+V で貼り付け（JPEG/PNG/PDF）"
                      >
                        📎 選択・D&D・貼り付け
                        <input
                          type="file"
                          accept="image/jpeg,image/png,application/pdf,.pdf"
                          className="sr-only"
                          onChange={(e) => {
                            const file = e.target.files?.[0];
                            e.target.value = "";
                            if (file) void attachReceiptFile(item, file);
                          }}
                        />
                      </label>
                      <button
                        type="button"
                        onClick={() => startMobileUpload(item)}
                        className="text-xs px-2.5 py-1 border border-border rounded-lg text-muted-foreground hover:text-foreground hover:border-foreground/30 transition-colors"
                      >
                        📱 スマホからアップロード
                      </button>
                    </div>
                    <button
                      type="button"
                      onClick={() => removeRow(item)}
                      className="shrink-0 text-muted-foreground hover:text-red-400 transition-colors text-sm"
                      title="行を削除"
                    >🗑</button>
                  </div>
                </div>
              ))}

              {items.length === 0 && (
                <p className="text-center text-sm text-muted-foreground py-6">「+ 行を追加」から明細を追加してください</p>
              )}
            </div>

            <div className="flex items-center justify-between pt-2 border-t border-border">
              <span className="text-sm text-muted-foreground">
                合計: <span className="text-foreground font-bold text-base">¥{totalAmount.toLocaleString()}</span>
              </span>
              <div className="flex gap-2">
                <button type="button" onClick={closeModal} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition-colors">キャンセル</button>
                <button
                  type="button"
                  onClick={() => saveMutation.mutate()}
                  disabled={saveMutation.isPending || items.length === 0}
                  className="px-4 py-2 bg-rose-600 hover:bg-rose-500 disabled:opacity-40 disabled:cursor-not-allowed text-foreground text-sm font-medium rounded-lg transition-colors"
                >
                  {saveMutation.isPending ? "保存中..." : "保存する"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* QRアップロードモーダル */}
      {qrItem && (
        <MobileUploadQrModal
          item={qrItem}
          onClose={() => setQrItem(null)}
          onUploaded={(receiptMime) => {
            setItems((prev) =>
              prev.map((it) =>
                it.localKey === qrItem.localKey
                  ? { ...it, has_receipt: true, receipt_mime: receiptMime ?? it.receipt_mime }
                  : it,
              ),
            );
            setQrItem(null);
          }}
        />
      )}

      {/* 削除確認ダイアログ */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="bg-card border border-border rounded-xl w-full max-w-sm p-6 space-y-4">
            <h2 className="text-lg font-bold text-foreground">削除確認</h2>
            <p className="text-sm text-muted-foreground">この経費申請を削除しますか？</p>
            <p className="text-sm text-foreground truncate">「{deleteTarget.label}」</p>
            <div className="flex justify-end gap-2 pt-2">
              <button onClick={() => setDeleteTarget(null)} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition-colors">キャンセル</button>
              <button
                onClick={() => deleteMutation.mutate(deleteTarget.id)}
                disabled={deleteMutation.isPending}
                className="px-4 py-2 bg-red-600 hover:bg-red-500 disabled:opacity-40 text-foreground text-sm font-medium rounded-lg transition-colors"
              >
                {deleteMutation.isPending ? "削除中..." : "削除する"}
              </button>
            </div>
          </div>
        </div>
      )}

      {detailEditId && (
        <DetailModal open title={`経費 #${detailEditId}`} icon="💳" size="xl" onClose={closeDetail}>
          <ExpenseDetailPage
            id={detailEditId}
            embedded
            onOpenFormEdit={openFormFromDetail}
          />
        </DetailModal>
      )}
    </div>
  );
}

/** スマホ連携アップロード用のQRコードモーダル。2秒間隔でアップロード完了をポーリングする */
function MobileUploadQrModal({
  item,
  onClose,
  onUploaded,
}: {
  item: LocalItem;
  onClose: () => void;
  onUploaded: (receiptMime?: string | null) => void;
}) {
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      if (item.id === null) return;
      const res = await issueMobileUploadToken(item.id);
      if (cancelled) return;
      if (!res.success || !res.upload_url || !res.token) {
        setError(res.error || "トークンの発行に失敗しました");
        return;
      }
      setToken(res.token);
      const dataUrl = await QRCode.toDataURL(res.upload_url, { width: 260, margin: 1 });
      if (!cancelled) setQrDataUrl(dataUrl);
    })();

    return () => { cancelled = true; };
  }, [item.id]);

  useEffect(() => {
    if (!token) return;

    pollRef.current = setInterval(async () => {
      const status = await fetchMobileUploadStatus(token);
      if (status.uploaded) {
        if (pollRef.current) clearInterval(pollRef.current);
        onUploaded(status.receipt_mime);
      } else if (status.expired) {
        if (pollRef.current) clearInterval(pollRef.current);
        setError("有効期限が切れました。もう一度お試しください");
      }
    }, 2000);

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70 backdrop-blur-sm">
      <div className="bg-card border border-border rounded-xl w-full max-w-xs p-6 space-y-4 text-center">
        <h3 className="text-base font-semibold text-foreground">スマホで撮影</h3>
        <p className="text-xs text-muted-foreground">
          スマホのカメラでQRコードを読み取り、領収書を撮影してください（有効期限10分）
        </p>
        {error ? (
          <p className="text-sm text-red-400 py-8">{error}</p>
        ) : qrDataUrl ? (
          <img src={qrDataUrl} alt="QRコード" className="mx-auto rounded-lg border border-border" />
        ) : (
          <div className="h-[260px] flex items-center justify-center text-muted-foreground text-sm">発行中...</div>
        )}
        <p className="text-[11px] text-muted-foreground">アップロードが完了すると自動的に閉じます</p>
        <button onClick={onClose} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition-colors">閉じる</button>
      </div>
    </div>
  );
}
