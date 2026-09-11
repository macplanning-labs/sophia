"use client";

import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import {
  fetchExpenseDetail, approveExpense, rejectExpense, unapproveExpense, resubmitExpense,
  expenseItemReceiptUrl, fetchCurrentUser,
} from "@/lib/api";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useDynamicId } from "@/lib/utils";
import { toast } from "sonner";
import { Check, Pencil, RotateCcw, Undo2, X } from "lucide-react";

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
  /** Embedded: open inline form edit on list page */
  onOpenFormEdit?: (id: string) => void;
}

export default function ExpenseDetailPage({ id: idProp, embedded = false, onOpenFormEdit }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const router = useRouter();
  const qc = useQueryClient();
  const [confirmUnapprove, setConfirmUnapprove] = useState(false);
  const { data, isLoading } = useQuery({
    queryKey: ["expenses", id],
    queryFn: () => fetchExpenseDetail(id),
    enabled: !!id,
  });
  const { data: currentUser } = useQuery({
    queryKey: ["current-user"],
    queryFn: fetchCurrentUser,
  });

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["expenses", id] });
    qc.invalidateQueries({ queryKey: ["expenses"] });
  };

  const approveMutation = useMutation({
    mutationFn: () => approveExpense(Number(id)),
    onSuccess: () => { invalidate(); toast.success("承認しました"); },
    onError: (e: Error) => toast.error(e.message || "承認に失敗しました"),
  });

  const rejectMutation = useMutation({
    mutationFn: () => rejectExpense(Number(id)),
    onSuccess: () => { invalidate(); toast.success("差戻しました"); },
    onError: (e: Error) => toast.error(e.message || "差戻しに失敗しました"),
  });

  const unapproveMutation = useMutation({
    mutationFn: () => unapproveExpense(Number(id)),
    onSuccess: () => {
      setConfirmUnapprove(false);
      invalidate();
      toast.success("承認を取り消し、申請中に戻しました");
    },
    onError: (e: Error) => toast.error(e.message || "承認取り消しに失敗しました"),
  });

  const resubmitMutation = useMutation({
    mutationFn: () => resubmitExpense(Number(id)),
    onSuccess: () => {
      invalidate();
      toast.success("再申請しました");
      if (embedded && onOpenFormEdit) {
        onOpenFormEdit(id);
      } else {
        router.push(`/expenses?form=${id}`);
      }
    },
    onError: (e: Error) => toast.error(e.message || "再申請に失敗しました"),
  });

  const exp = data?.expense;
  const items = data?.items ?? [];
  const canManage = !!currentUser?.can_view_all_expenses || currentUser?.role === "ADMIN";
  const canEditContent = exp?.status === "PENDING" || exp?.status === "DRAFT";

  return (
    <DetailLayout
      title={`経費 #${id}`}
      icon="💳"
      backHref="/expenses"
      backLabel="一覧に戻る"
      isLoading={isLoading}
      embedded={embedded}
      actions={
        <div className="flex items-center gap-2">
          {canEditContent && (
            <Button
              size="sm"
              variant="outline"
              className="text-muted-foreground border-border hover:bg-accent"
              onClick={() => {
                if (embedded && onOpenFormEdit) {
                  onOpenFormEdit(id);
                } else {
                  router.push(`/expenses?form=${id}`);
                }
              }}
            >
              <Pencil className="w-3.5 h-3.5 mr-1" /> 編集
            </Button>
          )}
          {exp?.status === "PENDING" && canManage && (
            <>
              <Button size="sm" variant="outline" className="text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/10" onClick={() => approveMutation.mutate()}>
                <Check className="w-3.5 h-3.5 mr-1" /> 承認
              </Button>
              <Button size="sm" variant="outline" className="text-red-400 border-red-500/30 hover:bg-red-500/10" onClick={() => rejectMutation.mutate()}>
                <X className="w-3.5 h-3.5 mr-1" /> 差戻し
              </Button>
            </>
          )}
          {exp?.status === "APPROVED" && canManage && (
            <Button
              size="sm"
              variant="outline"
              className="text-amber-400 border-amber-500/30 hover:bg-amber-500/10"
              onClick={() => setConfirmUnapprove(true)}
            >
              <Undo2 className="w-3.5 h-3.5 mr-1" /> 承認を取り消す
            </Button>
          )}
          {exp?.status === "REJECTED" && (
            <Button
              size="sm"
              variant="outline"
              className="text-sky-400 border-sky-500/30 hover:bg-sky-500/10"
              onClick={() => resubmitMutation.mutate()}
              disabled={resubmitMutation.isPending}
            >
              <RotateCcw className="w-3.5 h-3.5 mr-1" /> 再申請
            </Button>
          )}
        </div>
      }
    >
      {exp && (
        <>
          <FieldGrid>
            <Field label="申請者" value={data.employee_name} />
            <Field label="ステータス" value={
              <Badge className="text-[10px]">{data.status_display}</Badge>
            } />
            <Field label="合計金額" value={
              <span className="text-lg font-bold text-amber-400">¥{exp.total_amount.toLocaleString()}</span>
            } />
            <Field label="申請日" value={exp.created_at?.slice(0, 10)} />
          </FieldGrid>

          <div className="mt-6">
            <h3 className="text-sm font-semibold text-foreground mb-2">明細（{items.length}件）</h3>
            {!canEditContent && (
              <p className="text-xs text-muted-foreground mb-2">
                承認済・精算済の申請は閲覧のみです。内容や領収書の変更はできません。
              </p>
            )}
            <div className="bg-card border border-border rounded-lg overflow-hidden">
              <Table>
                <TableHeader>
                  <TableRow className="border-border hover:bg-transparent">
                    <TableHead className="text-xs text-muted-foreground">日付</TableHead>
                    <TableHead className="text-xs text-muted-foreground">カテゴリ</TableHead>
                    <TableHead className="text-xs text-muted-foreground">内容</TableHead>
                    <TableHead className="text-xs text-muted-foreground text-right">金額</TableHead>
                    <TableHead className="text-xs text-muted-foreground">領収書</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {items.length === 0 ? (
                    <TableRow><TableCell colSpan={5} className="text-center py-8 text-muted-foreground">明細がありません</TableCell></TableRow>
                  ) : items.map((item) => (
                    <TableRow key={item.id} className="border-border/50">
                      <TableCell className="text-sm text-muted-foreground">{item.expense_date}</TableCell>
                      <TableCell><Badge variant="outline" className="text-[10px] border-border">{item.category_display ?? item.category}</Badge></TableCell>
                      <TableCell className="text-sm text-muted-foreground max-w-64 truncate">{item.description || "—"}</TableCell>
                      <TableCell className="text-right text-sm tabular-nums font-medium">¥{item.amount.toLocaleString()}</TableCell>
                      <TableCell>
                        {item.has_receipt ? (
                          <a href={expenseItemReceiptUrl(item.id)} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5">
                            {item.receipt_mime === "application/pdf" ? (
                              <span className="inline-flex h-10 w-10 items-center justify-center rounded border border-border bg-muted text-[10px] font-semibold text-rose-300">
                                PDF
                              </span>
                            ) : (
                              <img src={expenseItemReceiptUrl(item.id)} alt="領収書" className="h-10 w-10 object-cover rounded border border-border" />
                            )}
                          </a>
                        ) : (
                          <span className="text-xs text-muted-foreground">未添付</span>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        </>
      )}

      {confirmUnapprove && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="bg-card border border-border rounded-xl w-full max-w-sm mx-4 p-6 space-y-4">
            <h2 className="text-lg font-bold text-foreground">承認を取り消しますか？</h2>
            <p className="text-sm text-muted-foreground">
              ステータスを「申請中」に戻します。申請者は再度内容・領収書を編集できるようになります。
              精算済の申請は取り消せません。
            </p>
            <div className="flex justify-end gap-2 pt-2">
              <Button variant="outline" size="sm" onClick={() => setConfirmUnapprove(false)} disabled={unapproveMutation.isPending}>
                キャンセル
              </Button>
              <Button
                size="sm"
                className="bg-amber-600 hover:bg-amber-500 text-foreground"
                onClick={() => unapproveMutation.mutate()}
                disabled={unapproveMutation.isPending}
              >
                {unapproveMutation.isPending ? "処理中..." : "申請中に戻す"}
              </Button>
            </div>
          </div>
        </div>
      )}
    </DetailLayout>
  );
}
