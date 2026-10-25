"use client";

import { useRouter } from "next/navigation";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, useEffect } from "react";
import { fetchUserDetail, apiPut, apiPost, apiDelete, fetchEmployeeOptions } from "@/lib/api";
import { HandOverDialog } from "@/components/org-admin/hand-over-dialog";
import { toggleUserWithTransfer, type OwnedCount, type TransferTo } from "@/lib/owner-transfer";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Pencil, KeyRound, ToggleLeft, ToggleRight, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useDynamicId } from "@/lib/utils";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { AccessDenied } from "@/components/access-denied";

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
  onDeleted?: () => void;
}

export default function UserDetailPage({ id: idProp, embedded = false, onDeleted }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const router = useRouter();
  const qc = useQueryClient();
  const { user: me, isAdmin, isLoading: userLoading } = useCurrentUser();
  // 自分自身は無効にできない(ログアウトして操作できなくなるため。DEMO-000186)
  const isSelf = me?.user_id === Number(id);

  const { data, isLoading } = useQuery({
    queryKey: ["users", id],
    queryFn: () => fetchUserDetail(id),
    enabled: !!id && isAdmin,
  });

  // ── 編集モーダル ──
  const [editOpen, setEditOpen] = useState(false);
  const [form, setForm] = useState<{ email: string; username: string; is_staff: boolean; can_view_all_payroll: boolean; can_view_all_expenses: boolean; employee_id: number | null }>({
    email: "", username: "", is_staff: false, can_view_all_payroll: false, can_view_all_expenses: false, employee_id: null,
  });

  const { data: employees } = useQuery({
    queryKey: ["employee-options"],
    queryFn: fetchEmployeeOptions,
  });

  useEffect(() => {
    if (data && editOpen) {
      setForm({
        email: data.email ?? "",
        username: data.username ?? "",
        is_staff: data.is_staff ?? false,
        can_view_all_payroll: data.can_view_all_payroll ?? false,
        can_view_all_expenses: data.can_view_all_expenses ?? false,
        employee_id: data.employee_id ?? null,
      });
    }
  }, [data, editOpen]);

  const updateMutation = useMutation({
    mutationFn: (d: typeof form) => apiPut(`/api/v1/users/${id}`, d),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users", id] });
      qc.invalidateQueries({ queryKey: ["users"] });
      setEditOpen(false);
      toast.success("ユーザー情報を更新しました");
    },
    onError: (e: Error) => toast.error(e.message || "更新に失敗しました"),
  });

  // ── パスワードリセット ──
  const [pwOpen, setPwOpen] = useState(false);
  const [newPassword, setNewPassword] = useState("");

  const resetPwMutation = useMutation({
    mutationFn: (pw: string) => apiPost(`/api/v1/users/${id}/reset-password`, { new_password: pw }),
    onSuccess: () => {
      setPwOpen(false);
      setNewPassword("");
      toast.success("パスワードをリセットしました");
    },
    onError: (e: Error) => toast.error(e.message || "リセットに失敗しました"),
  });

  // ── 有効/無効トグル ──
  // 担当データのある社員に結びつくユーザーを無効にすると 409。引き継ぎ先を選ぶ画面を開く。
  const [handOver, setHandOver] = useState<OwnedCount[] | null>(null);
  const toggleMutation = useMutation({
    mutationFn: (transferTo?: TransferTo) => toggleUserWithTransfer(Number(id), transferTo),
    onSuccess: (res) => {
      if (res.conflict) {
        setHandOver(res.owned);
        return;
      }
      setHandOver(null);
      qc.invalidateQueries({ queryKey: ["users", id] });
      qc.invalidateQueries({ queryKey: ["users"] });
      toast.success("ステータスを変更しました");
    },
    onError: (e: Error) => toast.error(e.message || "変更に失敗しました"),
  });

  // ── 削除 ──
  const [deleteOpen, setDeleteOpen] = useState(false);
  const deleteMutation = useMutation({
    mutationFn: () => apiDelete(`/api/v1/users/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users"] });
      toast.success("ユーザーを削除しました");
      if (embedded) {
        onDeleted?.();
      } else {
        router.push("/users");
      }
    },
    onError: (e: Error) => toast.error(e.message || "削除に失敗しました"),
  });

  if (userLoading) return null;
  if (!isAdmin) return <AccessDenied message="ユーザー管理を利用するには管理者権限が必要です。" />;

  return (
    <DetailLayout
      title={data?.username ?? `ユーザー #${id}`}
      icon="👥"
      backHref="/users"
      backLabel="一覧に戻る"
      isLoading={isLoading}
      embedded={embedded}
      actions={
        // 左 = 編集・PW変更・有効/無効の切り替え、右端 = 例外(削除)。ボタンは消さず、使えないときは非活性にする
        <div className="flex flex-1 items-center gap-2">
          <Button size="sm" variant="outline" onClick={() => setEditOpen(true)}>
            <Pencil className="w-3.5 h-3.5 mr-1" /> 編集
          </Button>
          <Button size="sm" variant="outline" onClick={() => setPwOpen(true)}>
            <KeyRound className="w-3.5 h-3.5 mr-1" /> PW変更
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={isSelf}
            title={isSelf ? "自分自身は無効にできません(ログアウトして操作できなくなるため)" : undefined}
            onClick={() => toggleMutation.mutate(undefined)}
            className={data?.is_active ? "text-amber-400 border-amber-500/30 hover:bg-amber-500/10" : "text-emerald-400 border-emerald-500/30 hover:bg-emerald-500/10"}
          >
            {data?.is_active ? <><ToggleLeft className="w-3.5 h-3.5 mr-1" /> 無効化</> : <><ToggleRight className="w-3.5 h-3.5 mr-1" /> 有効化</>}
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="border-red-700/50 text-red-400 hover:text-red-300 ml-auto"
            onClick={() => setDeleteOpen(true)}
          >
            <Trash2 className="w-3.5 h-3.5 mr-1" /> 削除
          </Button>
        </div>
      }
    >
      {data && (
        <>
          <FieldGrid>
            <Field label="ユーザー名" value={data.username} />
            <Field label="メール" value={data.email} />
            <Field label="ロール" value={
              <Badge variant="outline" className="text-[10px] border-border">{data.role_display}</Badge>
            } />
            <Field label="アクティブ" value={
              data.is_active
                ? <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">有効</Badge>
                : <Badge variant="outline" className="border-red-500/30 text-red-400 text-[10px]">無効</Badge>
            } />
          </FieldGrid>
          <FieldGrid>
            <Field label="MFA" value={data.mfa_enabled ? "有効" : "無効"} />
            <Field label="社員名" value={data.employee_name || "—"} />
            <Field label="パートナー" value={data.partner_name || "—"} />
            <Field label="作成日" value={data.created_at?.slice(0, 10)} />
          </FieldGrid>
          <FieldGrid>
            <Field label="給与閲覧権限" value={
              data.can_view_all_payroll
                ? <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">全社員閲覧可</Badge>
                : <Badge variant="outline" className="border-border text-[10px]">自分のみ</Badge>
            } />
            <Field label="経費閲覧権限" value={
              data.can_view_all_expenses
                ? <Badge className="bg-emerald-500/20 text-emerald-400 border-emerald-500/30 text-[10px]">全社員閲覧可</Badge>
                : <Badge variant="outline" className="border-border text-[10px]">自分のみ</Badge>
            } />
          </FieldGrid>
        </>
      )}

      {/* 編集モーダル */}
      <FormModal
        open={editOpen}
        onClose={() => setEditOpen(false)}
        title="ユーザー情報の編集"
        onSubmit={() => updateMutation.mutate(form)}
        loading={updateMutation.isPending}
      >
        <FormField label="ユーザー名" required>
          <FormInput value={form.username} onChange={(e) => setForm({ ...form, username: e.target.value })} />
        </FormField>
        <FormField label="メールアドレス" required>
          <FormInput value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} />
        </FormField>
        <FormField label="管理者権限">
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={form.is_staff}
              onChange={(e) => setForm({ ...form, is_staff: e.target.checked })}
              className="rounded border-border"
            />
            管理者として設定する
          </label>
        </FormField>
        <FormField label="給与閲覧権限">
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={form.can_view_all_payroll}
              onChange={(e) => setForm({ ...form, can_view_all_payroll: e.target.checked })}
              className="rounded border-border"
            />
            全社員の給与データを閲覧・確認・振込済み操作できるようにする
          </label>
          <p className="text-[11px] text-muted-foreground mt-1">
            管理者権限とは別の権限です。管理者であっても、これをONにしない限り他の社員の給与は見えません。
          </p>
        </FormField>
        <FormField label="経費閲覧権限">
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={form.can_view_all_expenses}
              onChange={(e) => setForm({ ...form, can_view_all_expenses: e.target.checked })}
              className="rounded border-border"
            />
            全社員の経費申請を閲覧・承認・差し戻しできるようにする
          </label>
          <p className="text-[11px] text-muted-foreground mt-1">
            管理者権限とは別の権限です。管理者であっても、これをONにしない限り他の社員の経費は見えません。
          </p>
        </FormField>
        <FormField label="紐付ける社員">
          <FormSelect
            value={form.employee_id ?? ""}
            onChange={(e) => setForm({ ...form, employee_id: e.target.value ? Number(e.target.value) : null })}
            placeholder="選択しない（社員紐付けなし）"
            options={(employees ?? []).map((emp) => ({ value: String(emp.id), label: emp.display_name }))}
          />
          <p className="text-[11px] text-muted-foreground mt-1">
            管理者権限をONにしない場合、ここで社員を紐付けないと「一般社員」ロールになれずログイン後の画面にアクセスできません。
          </p>
        </FormField>
      </FormModal>

      {/* パスワードリセットモーダル */}
      <FormModal
        open={pwOpen}
        onClose={() => { setPwOpen(false); setNewPassword(""); }}
        title="パスワードリセット"
        onSubmit={() => resetPwMutation.mutate(newPassword)}
        loading={resetPwMutation.isPending}
        submitLabel="リセット"
      >
        <FormField label="新しいパスワード" required>
          <FormInput value={newPassword} onChange={(e) => setNewPassword(e.target.value)} placeholder="新しいパスワードを入力" />
        </FormField>
      </FormModal>

      {/* 削除確認ダイアログ */}
      <ConfirmDialog
        open={deleteOpen}
        title="ユーザーを削除"
        description={`ユーザー ${data?.username ?? ""}（${data?.email ?? ""}）を削除します。この操作は取り消せません。よろしいですか？`}
        confirmLabel="削除する"
        variant="danger"
        loading={deleteMutation.isPending}
        onConfirm={() => deleteMutation.mutate()}
        onCancel={() => setDeleteOpen(false)}
      />

      {/* 担当データの引き継ぎ（無効化で 409 のとき） */}
      {handOver && (
        <HandOverDialog
          subjectLabel={`ユーザー ${data?.username ?? ""}`}
          owned={handOver}
          employeeId={data?.employee_id ?? null}
          submitLabel="引き継いで無効にする"
          loading={toggleMutation.isPending}
          onSubmit={(to) => toggleMutation.mutate(to)}
          onClose={() => setHandOver(null)}
        />
      )}
    </DetailLayout>
  );
}
