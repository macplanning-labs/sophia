"use client";

import { useCallback, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchMastersData, fetchMastersMeta, createMasterRecord, createClient } from "@/lib/api";
import { optionLabel, type MasterTableMeta } from "@/lib/master-form";
import { Button } from "@/components/ui/button";
import { PageHeader } from "@/components/ui/page-header";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FormModal } from "@/components/ui/form-modal";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Plus } from "lucide-react";
import PartyDetailPage from "@/app/parties/[id]/client";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { toast } from "sonner";
import { formatBillingUnit, formatOwner, validateClientOwner, buildClientCreatePayload, withPartyType } from "@/lib/parties-utils";
import { OrgEmployeePicker } from "@/components/org-admin/org-employee-picker";

type PartyType = "client" | "partner";

interface ClientRow {
  id: number;
  name: string;
  contact_person?: string;
  email?: string;
  billing_unit?: string;
  /** EDI方式のコード(表示名はマスタ定義の選択肢から引く) */
  edi_system_type?: string;
  /** 当社の担当組織・営業担当（一覧APIが JOIN で返す） */
  owner_org_name?: string | null;
  owner_employee_name?: string | null;
}

interface PartnerRow {
  partner_id: string;
  name: string;
  contact_person?: string;
  email?: string;
  [key: string]: unknown;
}

type PartyRow = ClientRow | PartnerRow;

/**
 * 取引先(クライアント・パートナー)の一覧・登録・編集。
 * 種別(?type=)と開く取引先(?open=)は URL で受け取る。
 */
export function PartiesPanel() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const queryClient = useQueryClient();
  const { isAdmin, isLoading: userLoading } = useCurrentUser();
  const mounted = useMounted();
  const showAdmin = isAdmin && mounted;

  const typeParam = searchParams.get("type") as PartyType | null;
  const [partyType, setPartyType] = useState<PartyType>(typeParam || "client");
  const [nameFilter, setNameFilter] = useState("");
  const [showCreateModal, setShowCreateModal] = useState(false);
  // ⌘K の検索結果から来たとき(?open=ID)は、その取引先の詳細を開く
  const [detailId, setDetailId] = useState<string | number | null>(searchParams.get("open"));
  const [createFormType, setCreateFormType] = useState<PartyType>("client");

  // Fetch data based on party type
  const tableKey = partyType === "client" ? "clients" : "partners";
  const { data: parties = [], isLoading } = useQuery({
    queryKey: ["masters-data", tableKey],
    queryFn: () => fetchMastersData(tableKey),
  });

  // EDI方式の表示名(マスタ定義の選択肢)
  const { data: masterTables = [] } = useQuery<MasterTableMeta[]>({
    queryKey: ["masters-meta"],
    queryFn: fetchMastersMeta,
    staleTime: 5 * 60 * 1000,
  });
  const ediCol = masterTables.find((t) => t.key === "clients")?.form_columns.find((c) => c.name === "edi_system_type");

  // Create mutation
  const createMutation = useMutation({
    mutationFn: async (payload: Record<string, unknown>) => {
      // クライアントは担当（組織・担当者）が必須のため専用API。パートナーは従来どおりマスタ汎用API
      if (createFormType === "client") return createClient(payload);
      return createMasterRecord("partners", payload);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["masters-data", "clients"] });
      queryClient.invalidateQueries({ queryKey: ["masters-data", "partners"] });
      setShowCreateModal(false);
      setCreateForm({
        name: "",
        contact_person: "",
        email: "",
        billing_unit: "PROJECT",
      });
      setOwnerOrgId("");
      setOwnerEmployeeId("");
      toast.success("取引先を登録しました");
    },
    onError: (err) => {
      toast.error(`登録に失敗しました: ${err instanceof Error ? err.message : "不明なエラー"}`);
    },
  });

  const [createForm, setCreateForm] = useState({
    name: "",
    contact_person: "",
    email: "",
    billing_unit: "PROJECT",
  });
  // クライアント登録の担当（組織・担当者）。select の value なので文字列、未選択は空
  const [ownerOrgId, setOwnerOrgId] = useState("");
  const [ownerEmployeeId, setOwnerEmployeeId] = useState("");
  const [ownerError, setOwnerError] = useState<string | undefined>(undefined);

  // Filter parties by name
  const filteredParties = useMemo(() => {
    if (!nameFilter) return parties;
    const lowerFilter = nameFilter.toLowerCase();
    return parties.filter((p: PartyRow) => {
      const name = (p.name || "").toLowerCase();
      return name.includes(lowerFilter);
    });
  }, [parties, nameFilter]);

  // Name options for SearchableColumnHeader
  const nameOptions = useMemo(() => {
    const map = new Map<string | number, string>();
    for (const p of parties) {
      const id = partyType === "client" ? (p as ClientRow).id : (p as PartnerRow).partner_id;
      const name = p.name || "";
      map.set(id, name);
    }
    return [...map.entries()]
      .sort((a, b) => a[1].localeCompare(b[1], "ja"))
      .map(([value, label]) => ({ value: String(value), label }));
  }, [parties, partyType]);

  const handleRowClick = useCallback(
    (party: PartyRow) => {
      const id = partyType === "client" ? (party as ClientRow).id : (party as PartnerRow).partner_id;
      setDetailId(id);
    },
    [partyType]
  );

  const handleCreateClick = () => {
    setCreateFormType(partyType);
    setCreateForm({ name: "", contact_person: "", email: "", billing_unit: "PROJECT" });
    setOwnerOrgId("");
    setOwnerEmployeeId("");
    setOwnerError(undefined);
    setShowCreateModal(true);
  };

  const handleSave = async () => {
    if (!createForm.name.trim()) {
      toast.error("名称は必須です");
      return;
    }

    if (createFormType === "client") {
      const problem = validateClientOwner(ownerOrgId, ownerEmployeeId);
      if (problem) {
        setOwnerError(problem);
        return;
      }
      setOwnerError(undefined);
      createMutation.mutate(buildClientCreatePayload(createForm, ownerOrgId, ownerEmployeeId));
      return;
    }

    createMutation.mutate({
      name: createForm.name,
      contact_person: createForm.contact_person || null,
      email: createForm.email || null,
    });
  };

  return (
    <div className="space-y-6">
      <PageHeader
        actions={
          showAdmin && !userLoading && (
            <Button size="sm" onClick={handleCreateClick} className="gap-1.5">
              <Plus className="w-3.5 h-3.5" /> 取引先登録
            </Button>
          )
        }
      />

      {/* Tabs */}
      <div className="flex gap-2 border-b border-border">
        <button
          className={`pb-3 px-1 text-sm font-medium border-b-2 transition-colors ${
            partyType === "client"
              ? "border-foreground text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
          onClick={() => {
            setPartyType("client");
            setNameFilter("");
            router.replace(withPartyType(searchParams, "client"), { scroll: false });
          }}
        >
          クライアント
        </button>
        <button
          className={`pb-3 px-1 text-sm font-medium border-b-2 transition-colors ${
            partyType === "partner"
              ? "border-foreground text-foreground"
              : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
          onClick={() => {
            setPartyType("partner");
            setNameFilter("");
            router.replace(withPartyType(searchParams, "partner"), { scroll: false });
          }}
        >
          パートナー
        </button>
      </div>

      {/* Table */}
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="名称"
                    options={nameOptions}
                    value={nameFilter}
                    onChange={setNameFilter}
                    placeholder="名称で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">先方の担当者</TableHead>
                {partyType === "client" && <TableHead className="text-xs text-muted-foreground">当社の営業担当</TableHead>}
                <TableHead className="text-xs text-muted-foreground">メール</TableHead>
                {partyType === "client" && (
                  <>
                    <TableHead className="text-xs text-muted-foreground">請求単位</TableHead>
                    <TableHead className="text-xs text-muted-foreground">EDI方式</TableHead>
                  </>
                )}
              </TableRow>
            </TableHeader>
            <TableBody>
              {parties.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={partyType === "client" ? 6 : 3} className="text-center py-12 text-muted-foreground">
                    データがありません
                  </TableCell>
                </TableRow>
              ) : filteredParties.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={partyType === "client" ? 6 : 3} className="text-center py-12 text-muted-foreground">
                    条件に一致するデータがありません
                  </TableCell>
                </TableRow>
              ) : (
                filteredParties.map((party: PartyRow) => {
                  const id = partyType === "client" ? (party as ClientRow).id : (party as PartnerRow).partner_id;
                  return (
                    <TableRow
                      key={id}
                      className="border-border/50 cursor-pointer hover:bg-accent/50"
                      onClick={() => handleRowClick(party)}
                    >
                      <TableCell className="text-sm font-medium">{party.name}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{party.contact_person || "—"}</TableCell>
                      {partyType === "client" && (
                        <TableCell className="text-sm text-muted-foreground">
                          {formatOwner((party as ClientRow).owner_org_name, (party as ClientRow).owner_employee_name)}
                        </TableCell>
                      )}
                      <TableCell className="text-[11px] text-muted-foreground">{party.email || "—"}</TableCell>
                      {partyType === "client" && (
                        <>
                          <TableCell className="text-sm text-muted-foreground">{formatBillingUnit((party as ClientRow).billing_unit)}</TableCell>
                          <TableCell className="text-sm text-muted-foreground">{optionLabel(ediCol, (party as ClientRow).edi_system_type)}</TableCell>
                        </>
                      )}
                    </TableRow>
                  );
                })
              )}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredParties.length}件</span>
        </div>
      </div>

      {/* Create Modal */}
      {showAdmin && (
        <FormModal
          open={showCreateModal}
          title={`${createFormType === "client" ? "クライアント" : "パートナー"}登録`}
          size="lg"
          loading={createMutation.isPending}
          submitLabel="登録"
          onSubmit={handleSave}
          onClose={() => setShowCreateModal(false)}
        >
          <div className="grid grid-cols-1 gap-4">
            <div>
              <label className="block text-xs text-muted-foreground mb-1">種別 *</label>
              <div className="flex gap-2">
                <button
                  className={`flex-1 px-3 py-2 rounded-lg border text-sm font-medium transition-colors ${
                    createFormType === "client"
                      ? "border-foreground bg-foreground/10 text-foreground"
                      : "border-border text-muted-foreground hover:text-foreground"
                  }`}
                  onClick={() => setCreateFormType("client")}
                >
                  クライアント
                </button>
                <button
                  className={`flex-1 px-3 py-2 rounded-lg border text-sm font-medium transition-colors ${
                    createFormType === "partner"
                      ? "border-foreground bg-foreground/10 text-foreground"
                      : "border-border text-muted-foreground hover:text-foreground"
                  }`}
                  onClick={() => setCreateFormType("partner")}
                >
                  パートナー
                </button>
              </div>
            </div>
            <div>
              <label className="block text-xs text-muted-foreground mb-1">名称 *</label>
              <input
                className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                value={createForm.name}
                onChange={(e) => setCreateForm({ ...createForm, name: e.target.value })}
                placeholder="例: 株式会社サンプル"
              />
            </div>
            <div>
              <label className="block text-xs text-muted-foreground mb-1">先方の担当者</label>
              <input
                className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                value={createForm.contact_person}
                onChange={(e) => setCreateForm({ ...createForm, contact_person: e.target.value })}
                placeholder="例: 山田太郎"
              />
            </div>
            <div>
              <label className="block text-xs text-muted-foreground mb-1">メール</label>
              <input
                type="email"
                className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                value={createForm.email}
                onChange={(e) => setCreateForm({ ...createForm, email: e.target.value })}
                placeholder="例: sample@example.com"
              />
            </div>
            {createFormType === "client" && (
              <div>
                <label className="block text-xs text-muted-foreground mb-1">請求単位</label>
                <select
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground"
                  value={createForm.billing_unit}
                  onChange={(e) => setCreateForm({ ...createForm, billing_unit: e.target.value })}
                >
                  <option value="PROJECT">案件ごと</option>
                  <option value="CLIENT">取引先まとめ</option>
                </select>
              </div>
            )}
            {createFormType === "client" && (
              <>
                <OrgEmployeePicker
                  orgId={ownerOrgId}
                  employeeId={ownerEmployeeId}
                  onOrgChange={setOwnerOrgId}
                  onEmployeeChange={setOwnerEmployeeId}
                  error={ownerError}
                />
                <p className="text-[11px] text-muted-foreground">
                  この取引先の案件・契約・受注・請求書などは、ここで選んだ担当になります
                </p>
              </>
            )}
          </div>
        </FormModal>
      )}

      {/* Detail Modal */}
      {detailId !== null && (
        <DetailModal
          open
          title={`取引先 #${detailId}`}
          icon="🏢"
          size="xl"
          onClose={() => setDetailId(null)}
        >
          {/* 一覧の行から開いたら、そのまま編集できる(管理者)。保存したら閉じて一覧に戻る */}
          <PartyDetailPage id={String(detailId)} type={partyType} embedded editInline onSaved={() => setDetailId(null)} />
        </DetailModal>
      )}
    </div>
  );
}
