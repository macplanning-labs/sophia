"use client";

import { useState, useMemo, useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchMastersMeta, fetchMastersData, fetchMasterDetail, fetchMasterFkOptions, createMasterRecord, updateMasterRecord, deleteMasterRecord } from "@/lib/api";
import { Plus, Pencil, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { PageHeader } from "@/components/ui/page-header";
import { DataTableWrapper } from "@/components/ui/data-table-wrapper";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FormModal, FormField, FormInput, FormSelect, FormTextarea } from "@/components/ui/form-modal";
import { toast } from "sonner";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { AccessDenied } from "@/components/access-denied";
import { CompanyInfoPanel } from "./company-info-panel";
import { cn } from "@/lib/utils";

// ── 型定義 ──

interface ColumnMeta {
  name: string;
  label: string;
}

interface FormColumnMeta {
  name: string;
  label: string;
  input_type: string;
  required: boolean;
  is_pk: boolean;
  options?: { value: string; label: string }[];
  fk_table?: string;
  fk_value?: string;
  fk_label?: string;
}

interface TableMeta {
  key: string;
  label: string;
  category: string;
  pk_column: string;
  list_columns: ColumnMeta[];
  form_columns: FormColumnMeta[];
}

// バックエンドのMASTER_TABLES一覧には乗せず、フロント側だけで特別扱いするタブ
// （s_company_infoは単一行+機密情報を含むため、汎用の一覧CRUDエンジンに乗せず専用フォームで表示）
const COMPANY_INFO_TAB_KEY = "company_info";
const COMPANY_INFO_TAB = { key: COMPANY_INFO_TAB_KEY, label: "自社情報", category: "システム設定" };

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Row = Record<string, any>;

// ── メインコンポーネント ──

export default function MastersPage() {
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState("");
  const [modalMode, setModalMode] = useState<"closed" | "create" | "edit">("closed");
  const [editId, setEditId] = useState<string>("");
  const [formData, setFormData] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const { isAdmin, isLoading: userLoading } = useCurrentUser();

  // メタ情報（キャッシュ: 長め）
  const { data: tables = [] } = useQuery<TableMeta[]>({
    queryKey: ["masters-meta"],
    queryFn: fetchMastersMeta,
    staleTime: 5 * 60 * 1000,
    enabled: isAdmin,
  });

  // activeTabの初期設定
  const effectiveTab = activeTab || (tables.length > 0 ? tables[0].key : "");
  const isCompanyInfoTab = effectiveTab === COMPANY_INFO_TAB_KEY;

  // カテゴリ別グループ化（タブバー表示用。会社情報タブも同列に混ぜる）
  const groupedTabs = useMemo(() => {
    const groups = new Map<string, { key: string; label: string }[]>();
    for (const t of [...tables, COMPANY_INFO_TAB]) {
      const list = groups.get(t.category) ?? [];
      list.push({ key: t.key, label: t.label });
      groups.set(t.category, list);
    }
    return Array.from(groups.entries());
  }, [tables]);

  // データ取得（タブごとにキャッシュ。会社情報タブは専用パネル側で取得するのでスキップ）
  const { data: rows = [], isLoading: rowsLoading } = useQuery<Row[]>({
    queryKey: ["masters-data", effectiveTab],
    queryFn: () => fetchMastersData(effectiveTab),
    enabled: !!effectiveTab && isAdmin && !isCompanyInfoTab,
    staleTime: 30 * 1000,
  });

  const activeMeta = useMemo(
    () => tables.find((t) => t.key === effectiveTab),
    [tables, effectiveTab]
  );

  // FK選択肢（クライアント・パートナー等のドロップダウン用）
  const hasFkFields = !!activeMeta?.form_columns.some((c) => c.input_type === "fk_select");
  const { data: fkOptions = {} } = useQuery<Record<string, { value: string; label: string }[]>>({
    queryKey: ["masters-fk-options", effectiveTab],
    queryFn: () => fetchMasterFkOptions(effectiveTab),
    enabled: !!effectiveTab && isAdmin && hasFkFields,
    staleTime: 60 * 1000,
  });

  // ── 行クリック → 編集モーダル ──
  const handleRowClick = useCallback(async (row: Row) => {
    if (!activeMeta) return;
    const id = String(row[activeMeta.pk_column] ?? "");
    if (!id) return;
    try {
      const detail = await fetchMasterDetail(effectiveTab, id);
      const fd: Record<string, string> = {};
      activeMeta.form_columns.forEach((col) => {
        const val = detail[col.name];
        fd[col.name] = val === null || val === undefined ? "" : String(val);
      });
      setFormData(fd);
      setEditId(id);
      setModalMode("edit");
    } catch (e) {
      console.error("fetch detail:", e);
    }
  }, [activeMeta, effectiveTab]);

  // ── 新規作成 ──
  const handleCreate = useCallback(() => {
    if (!activeMeta) return;
    const fd: Record<string, string> = {};
    activeMeta.form_columns.forEach((col) => {
      fd[col.name] = "";
    });
    setFormData(fd);
    setEditId("");
    setModalMode("create");
  }, [activeMeta]);

  // ── 保存 ──
  const handleSave = useCallback(async () => {
    if (!activeMeta) return;
    setSaving(true);
    try {
      // boolean変換
      const payload: Record<string, unknown> = {};
      activeMeta.form_columns.forEach((col) => {
        const v = formData[col.name] ?? "";
        if (col.input_type === "checkbox") {
          payload[col.name] = v === "true" || v === "on";
        } else if (col.input_type === "number" && v !== "") {
          payload[col.name] = Number(v);
        } else {
          payload[col.name] = v;
        }
      });

      if (modalMode === "create") {
        await createMasterRecord(effectiveTab, payload);
      } else {
        await updateMasterRecord(effectiveTab, editId, payload);
      }
      queryClient.invalidateQueries({ queryKey: ["masters-data", effectiveTab] });
      setModalMode("closed");
    } catch (e) {
      console.error("save:", e);
      toast.error("保存に失敗しました");
    } finally {
      setSaving(false);
    }
  }, [activeMeta, formData, modalMode, editId, effectiveTab, queryClient]);

  // ── 削除 ──
  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      await deleteMasterRecord(effectiveTab, deleteTarget);
      queryClient.invalidateQueries({ queryKey: ["masters-data", effectiveTab] });
      setDeleteTarget(null);
      setModalMode("closed");
    } catch (e) {
      console.error("delete:", e);
      toast.error("削除に失敗しました");
    }
  }, [deleteTarget, effectiveTab, queryClient]);

  // ── フォーム値変更 ──
  const updateField = useCallback((name: string, value: string) => {
    setFormData((prev) => ({ ...prev, [name]: value }));
  }, []);

  const listCols = activeMeta?.list_columns ?? [];

  if (userLoading) return null;
  if (!isAdmin) return <AccessDenied message="マスタメンテナンスを利用するには管理者権限が必要です。" />;

  return (
    <div className="p-6">
      <PageHeader
        actions={
          activeMeta ? (
            <Button
              size="sm"
              className="bg-primary hover:bg-primary/90 text-primary-foreground gap-1"
              onClick={handleCreate}
            >
              <Plus className="w-4 h-4" /> 新規作成
            </Button>
          ) : undefined
        }
      />

      {/* タブ（カテゴリ別グループ表示） */}
      <div className="space-y-2 border-b border-border pb-3 mb-4">
        {groupedTabs.map(([category, items]) => (
          <div key={category} className="flex items-center gap-3 flex-wrap">
            <span className="text-[11px] text-muted-foreground uppercase tracking-wider w-24 shrink-0">
              {category}
            </span>
            <div className="flex gap-1 flex-wrap">
              {items.map((t) => (
                <button
                  key={t.key}
                  type="button"
                  onClick={() => setActiveTab(t.key)}
                  className={cn(
                    "px-3 py-1.5 rounded-md text-sm whitespace-nowrap transition-colors",
                    effectiveTab === t.key
                      ? "bg-primary/10 text-primary font-medium border border-primary/30"
                      : "text-muted-foreground hover:text-foreground hover:bg-muted opacity-70 hover:opacity-100"
                  )}
                >
                  {t.label}
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>

      {/* 自社情報パネル（単一行・専用フォーム） */}
      {isCompanyInfoTab && <CompanyInfoPanel />}

      {/* データテーブル */}
      {activeMeta && (
        <DataTableWrapper
          isLoading={rowsLoading}
          isEmpty={!rowsLoading && rows.length === 0}
          count={rows.length}
        >
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                {listCols.map((col) => (
                  <TableHead key={col.name} className="text-xs text-muted-foreground">
                    {col.label}
                  </TableHead>
                ))}
                <TableHead className="w-20" />
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((row, i) => {
                const pkVal = String(row[activeMeta.pk_column] ?? i);
                return (
                  <TableRow
                    key={pkVal}
                    className="border-border/50 hover:bg-accent/50 cursor-pointer"
                    onClick={() => handleRowClick(row)}
                  >
                    {listCols.map((col) => (
                      <TableCell key={col.name} className="text-sm">
                        {formatCellValue(row[col.name])}
                      </TableCell>
                    ))}
                    <TableCell>
                      <div className="flex gap-1">
                        <button
                          type="button"
                          className="p-1 text-muted-foreground hover:text-primary transition-colors"
                          onClick={(e) => { e.stopPropagation(); handleRowClick(row); }}
                          title="編集"
                        >
                          <Pencil className="w-3.5 h-3.5" />
                        </button>
                        <button
                          type="button"
                          className="p-1 text-muted-foreground hover:text-red-400 transition-colors"
                          onClick={(e) => { e.stopPropagation(); setDeleteTarget(pkVal); }}
                          title="削除"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </DataTableWrapper>
      )}

      {/* 編集/新規モーダル */}
      {activeMeta && (
        <FormModal
          open={modalMode !== "closed"}
          title={modalMode === "create" ? `${activeMeta.label} — 新規作成` : `${activeMeta.label} — 編集`}
          loading={saving}
          onSubmit={handleSave}
          onClose={() => setModalMode("closed")}
          footerStart={
            modalMode === "edit" ? (
              <Button
                variant="outline"
                size="sm"
                className="border-red-900/50 text-red-400 hover:bg-red-950/50 hover:text-red-300"
                onClick={() => setDeleteTarget(editId)}
              >
                <Trash2 className="w-3.5 h-3.5 mr-1" /> 削除
              </Button>
            ) : undefined
          }
        >
          {activeMeta.form_columns.map((col) => (
            <DynamicField
              key={col.name}
              col={col.input_type === "fk_select" ? { ...col, options: fkOptions[col.name] ?? [] } : col}
              value={formData[col.name] ?? ""}
              onChange={(v) => updateField(col.name, v)}
            />
          ))}
        </FormModal>
      )}

      {/* 削除確認 */}
      <ConfirmDialog
        open={!!deleteTarget}
        title="削除の確認"
        description={`このレコードを削除しますか？この操作は取り消せません。`}
        confirmLabel="削除"
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}

// ── セル表示 ──

function formatCellValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "boolean") return value ? "✓" : "—";
  return String(value);
}

// ── 動的フォームフィールド ──

function DynamicField({
  col,
  value,
  onChange,
}: {
  col: FormColumnMeta;
  value: string;
  onChange: (v: string) => void;
}) {
  if (col.input_type === "select" || col.input_type === "fk_select") {
    return (
      <FormField label={col.label} required={col.required}>
        <FormSelect
          value={value}
          onChange={(e) => onChange(e.target.value)}
          options={[
            { value: "", label: "— 選択 —" },
            ...(col.options ?? []),
          ]}
        />
      </FormField>
    );
  }

  if (col.input_type === "checkbox") {
    return (
      <FormField label={col.label} required={col.required}>
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={value === "true" || value === "on"}
            onChange={(e) => onChange(e.target.checked ? "true" : "false")}
            className="w-4 h-4 rounded border-border bg-muted text-primary focus:ring-primary/50"
          />
          <span className="text-sm text-foreground">有効</span>
        </label>
      </FormField>
    );
  }

  if (col.input_type === "textarea") {
    return (
      <FormField label={col.label} required={col.required}>
        <FormTextarea value={value} onChange={(e) => onChange(e.target.value)} rows={3} />
      </FormField>
    );
  }

  return (
    <FormField label={col.label} required={col.required}>
      <FormInput
        type={col.input_type === "number" ? "number" : col.input_type === "email" ? "email" : col.input_type === "date" ? "date" : "text"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        required={col.required}
      />
    </FormField>
  );
}
