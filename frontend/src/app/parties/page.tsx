"use client";

import { Suspense, useCallback, useMemo, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchMastersData, createMasterRecord } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { FormModal } from "@/components/ui/form-modal";
import { DetailModal } from "@/components/ui/detail-modal";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Plus } from "lucide-react";
import PartyDetailPage from "./[id]/client";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { toast } from "sonner";
import { formatBillingUnit } from "@/lib/parties-utils";

type PartyType = "client" | "partner";

interface ClientRow {
  id: number;
  name: string;
  contact_person?: string;
  email?: string;
  billing_unit?: string;
  edi_format?: string;
}

interface PartnerRow {
  partner_id: string;
  name: string;
  contact_person?: string;
  email?: string;
  [key: string]: unknown;
}

type PartyRow = ClientRow | PartnerRow;

export default function PartiesPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <PartiesPageContent />
    </Suspense>
  );
}

function PartiesPageContent() {
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
  const [detailId, setDetailId] = useState<string | number | null>(null);
  const [createFormType, setCreateFormType] = useState<PartyType>("client");

  // Fetch data based on party type
  const tableKey = partyType === "client" ? "clients" : "partners";
  const { data: parties = [], isLoading } = useQuery({
    queryKey: ["masters-data", tableKey],
    queryFn: () => fetchMastersData(tableKey),
  });

  // Create mutation
  const createMutation = useMutation({
    mutationFn: async (payload: Record<string, unknown>) => {
      const table = createFormType === "client" ? "clients" : "partners";
      return createMasterRecord(table, payload);
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
    setShowCreateModal(true);
  };

  const handleSave = async () => {
    if (!createForm.name.trim()) {
      toast.error("名称は必須です");
      return;
    }

    const payload: Record<string, unknown> = {
      name: createForm.name,
      contact_person: createForm.contact_person || null,
      email: createForm.email || null,
    };

    if (createFormType === "client") {
      payload.billing_unit = createForm.billing_unit;
    }

    createMutation.mutate(payload);
  };

  return (
    <div className="p-6 space-y-6">
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
            router.replace("?type=client", { scroll: false });
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
            router.replace("?type=partner", { scroll: false });
          }}
        >
          パートナー
        </button>
      </div>

      {/* Action bar */}
      <div className="flex items-center justify-end">
        {showAdmin && !userLoading && (
          <Button size="sm" onClick={handleCreateClick} className="gap-1.5">
            <Plus className="w-3.5 h-3.5" /> 取引先登録
          </Button>
        )}
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
                <TableHead className="text-xs text-muted-foreground">担当者</TableHead>
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
                  <TableCell colSpan={partyType === "client" ? 5 : 3} className="text-center py-12 text-muted-foreground">
                    データがありません
                  </TableCell>
                </TableRow>
              ) : filteredParties.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={partyType === "client" ? 5 : 3} className="text-center py-12 text-muted-foreground">
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
                      <TableCell className="text-[11px] text-muted-foreground">{party.email || "—"}</TableCell>
                      {partyType === "client" && (
                        <>
                          <TableCell className="text-sm text-muted-foreground">{formatBillingUnit((party as ClientRow).billing_unit)}</TableCell>
                          <TableCell className="text-sm text-muted-foreground">{(party as ClientRow).edi_format || "—"}</TableCell>
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
              <label className="block text-xs text-muted-foreground mb-1">担当者</label>
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
          <PartyDetailPage id={String(detailId)} type={partyType} embedded />
        </DetailModal>
      )}
    </div>
  );
}
