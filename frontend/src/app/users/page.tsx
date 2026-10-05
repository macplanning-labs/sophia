"use client";

import { Suspense, useCallback, useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchUsers, apiPost, fetchEmployeeOptions } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FormModal, FormField, FormInput, FormSelect } from "@/components/ui/form-modal";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { DetailModal } from "@/components/ui/detail-modal";
import { Plus, ToggleLeft, ToggleRight } from "lucide-react";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { AccessDenied } from "@/components/access-denied";
import UserDetailPage from "./[id]/client";

interface CreateForm {
  email: string;
  username: string;
  password: string;
  is_staff: boolean;
  employee_id: number | null;
}

const EMPTY_FORM: CreateForm = {
  email: "", username: "", password: "", is_staff: false, employee_id: null,
};

export default function UsersPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <UsersPageContent />
    </Suspense>
  );
}

function UsersPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const qc = useQueryClient();
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const [editId, setEditId] = useState<string | null>(null);

  const { data: users, isLoading } = useQuery({
    queryKey: ["users"],
    queryFn: fetchUsers,
    enabled: isAdmin,
  });

  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setEditId(fromUrl);
  }, [searchParams]);

  const openEdit = useCallback((id: string | number) => {
    const idStr = String(id);
    setEditId(idStr);
    router.replace(`/users?edit=${encodeURIComponent(idStr)}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/users", { scroll: false });
  }, [router]);

  // ── 新規作成モーダル ──
  const [modalOpen, setModalOpen] = useState(false);
  const [form, setForm] = useState<CreateForm>(EMPTY_FORM);

  const { data: employees } = useQuery({
    queryKey: ["employee-options"],
    queryFn: fetchEmployeeOptions,
    enabled: isAdmin,
  });

  const createMutation = useMutation({
    mutationFn: (data: CreateForm) => apiPost("/api/v1/users", data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users"] });
      setModalOpen(false);
      setForm(EMPTY_FORM);
      toast.success("ユーザーを作成しました");
    },
    onError: (e: Error) => toast.error(e.message || "作成に失敗しました"),
  });

  // ── 有効/無効トグル ──
  const toggleMutation = useMutation({
    mutationFn: (id: number) => apiPost(`/api/v1/users/${id}/toggle`, {}),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["users"] });
      toast.success("ステータスを変更しました");
    },
    onError: (e: Error) => toast.error(e.message || "変更に失敗しました"),
  });

  const handleToggle = (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    toggleMutation.mutate(id);
  };

  if (userLoading) return null;
  if (!isAdmin) return <AccessDenied message="ユーザー管理を利用するには管理者権限が必要です。" />;

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <Button size="sm" onClick={() => setModalOpen(true)}>
          <Plus className="w-4 h-4 mr-1" /> 新規作成
        </Button>
      </div>

      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">ID</TableHead>
                <TableHead className="text-xs text-muted-foreground">ユーザー名</TableHead>
                <TableHead className="text-xs text-muted-foreground">メール</TableHead>
                <TableHead className="text-xs text-muted-foreground">ロール</TableHead>
                <TableHead className="text-xs text-muted-foreground">紐付き</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">MFA</TableHead>
                <TableHead className="text-xs text-muted-foreground text-center">状態</TableHead>
                <TableHead className="text-xs text-muted-foreground">登録日</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(users ?? []).length === 0 ? (
                <TableRow><TableCell colSpan={8} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : users?.map((u) => (
                <TableRow key={u.id} className="border-border/50 cursor-pointer hover:bg-accent/50" onClick={() => openEdit(u.id)}>
                  <TableCell className="text-sm tabular-nums text-muted-foreground">{u.id}</TableCell>
                  <TableCell className="text-sm font-medium">{u.username}</TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">{u.email}</TableCell>
                  <TableCell><Badge variant="outline" className="text-[10px] border-border">{u.role_display ?? "—"}</Badge></TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">
                    {u.employee_name && `社員: ${u.employee_name}`}
                    {u.partner_name && `パートナー: ${u.partner_name}`}
                    {!u.employee_name && !u.partner_name && "—"}
                  </TableCell>
                  <TableCell className="text-center">
                    {u.mfa_enabled ? <Badge className="bg-blue-500/20 text-blue-400 border-blue-500/30 text-[10px]">有効</Badge>
                     : <Badge variant="outline" className="border-border text-muted-foreground text-[10px]">無効</Badge>}
                  </TableCell>
                  <TableCell className="text-center">
                    <button onClick={(e) => handleToggle(e, u.id)} className="inline-flex items-center gap-1 group" title="クリックで切替">
                      {u.is_active ? (
                        <><ToggleRight className="w-5 h-5 text-emerald-400 group-hover:text-emerald-300" /><span className="text-[10px] text-emerald-400">有効</span></>
                      ) : (
                        <><ToggleLeft className="w-5 h-5 text-muted-foreground group-hover:text-foreground" /><span className="text-[10px] text-muted-foreground">無効</span></>
                      )}
                    </button>
                  </TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">{u.created_at?.slice(0, 10)}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {users?.length ?? 0}件</span>
        </div>
      </div>

      {/* 新規作成モーダル */}
      <FormModal
        open={modalOpen}
        onClose={() => { setModalOpen(false); setForm(EMPTY_FORM); }}
        title="ユーザー新規作成"
        onSubmit={() => createMutation.mutate(form)}
        loading={createMutation.isPending}
      >
        <FormField label="ユーザー名" required>
          <FormInput value={form.username} onChange={(e) => setForm({ ...form, username: e.target.value })} placeholder="例: 田中太郎" />
        </FormField>
        <FormField label="メールアドレス" required>
          <FormInput value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} placeholder="例: tanaka@example.com" />
        </FormField>
        <FormField label="パスワード" required>
          <FormInput value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })} placeholder="初期パスワード" />
        </FormField>
        <FormField label="管理者権限">
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input
              type="checkbox"
              checked={form.is_staff}
              onChange={(e) => setForm({ ...form, is_staff: e.target.checked })}
              className="rounded border-border"
            />
            管理者として登録する
          </label>
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

      {editId && (
        <DetailModal
          open
          title={(users ?? []).find((u) => String(u.id) === editId)?.username ?? `ユーザー #${editId}`}
          icon="👥"
          size="xl"
          onClose={closeEdit}
        >
          <UserDetailPage id={editId} embedded onDeleted={closeEdit} />
        </DetailModal>
      )}
    </div>
  );
}
