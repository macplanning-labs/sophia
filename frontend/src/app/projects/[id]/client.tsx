"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useQuery } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { fetchProjectDetail, fetchReceivedOrders, fetchOrders } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { ProjectEditModal } from "@/components/projects/ProjectEditModal";
import { ClientContractEditModal } from "@/components/contracts/client-contract-edit-modal";
import { PartnerContractEditModal } from "@/components/contracts/partner-contract-edit-modal";

type TabKey = "info" | "client" | "partner" | "received" | "orders";
const TABS: { key: TabKey; label: string }[] = [
  { key: "info", label: "案件情報" },
  { key: "client", label: "受注契約" },
  { key: "partner", label: "発注契約" },
  { key: "received", label: "注文書(受注)履歴" },
  { key: "orders", label: "発注書履歴" },
];

interface ClientContractSummary { id: number; engineer_name: string; start_date: string; end_date: string; is_active: boolean }
interface PartnerContractSummary extends ClientContractSummary { partner_name: string }

function Empty({ text }: { text: string }) {
  return <p className="text-sm text-muted-foreground py-8 text-center">{text}</p>;
}

export default function ProjectDetailPage() {
  const projectId = useDynamicId();
  const router = useRouter();
  const [tab, setTab] = useState<TabKey>("info");
  const [editOpen, setEditOpen] = useState(false);
  const [clientContractId, setClientContractId] = useState<number | null>(null);
  const [partnerContractId, setPartnerContractId] = useState<number | null>(null);

  const { data: detail, isLoading } = useQuery({
    queryKey: ["projects", projectId],
    queryFn: () => fetchProjectDetail(projectId),
    enabled: !!projectId,
  });
  const project = detail?.project;
  const clientContracts: ClientContractSummary[] = detail?.client_contracts ?? [];
  const partnerContracts: PartnerContractSummary[] = detail?.partner_contracts ?? [];

  const { data: receivedAll } = useQuery({ queryKey: ["received-orders"], queryFn: fetchReceivedOrders, enabled: tab === "received" });
  const { data: ordersAll } = useQuery({ queryKey: ["orders", {}], queryFn: () => fetchOrders(), enabled: tab === "orders" });

  // 注文書の行にはプロジェクトIDが無いため、案件名（とクライアント名）で絞り込む
  const receivedOrders = useMemo(
    () => (receivedAll ?? []).filter((r) => r.project_name === project?.name && r.client_name === project?.client_name),
    [receivedAll, project],
  );
  const orders = useMemo(
    () => (ordersAll ?? []).filter((o) => o.project_name === project?.name),
    [ordersAll, project],
  );

  if (isLoading || !project) {
    return <div className="p-6 text-sm text-muted-foreground">{isLoading ? "読み込み中..." : "案件が見つかりません"}</div>;
  }

  const contractTable = (
    rows: (ClientContractSummary | PartnerContractSummary)[],
    onOpen: (id: number) => void,
    addHref: string,
    withPartner: boolean,
  ) => (
    <div className="space-y-3">
      <div className="flex justify-end">
        <Button size="sm" className="bg-blue-600 hover:bg-blue-700 gap-1" onClick={() => router.push(addHref)}>
          <Plus className="w-4 h-4" /> 追加
        </Button>
      </div>
      {rows.length === 0 ? <Empty text="契約はまだありません" /> : (
        <Table>
          <TableHeader>
            <TableRow>
              {withPartner && <TableHead>パートナー</TableHead>}
              <TableHead>技術者</TableHead><TableHead>期間</TableHead><TableHead className="text-center">状態</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((c) => (
              <TableRow key={c.id} className="cursor-pointer hover:bg-accent/50" onClick={() => onOpen(c.id)}>
                {withPartner && <TableCell className="text-sm">{(c as PartnerContractSummary).partner_name}</TableCell>}
                <TableCell className="text-sm text-emerald-400">{c.engineer_name}</TableCell>
                <TableCell className="text-sm text-muted-foreground">{c.start_date} 〜 {c.end_date}</TableCell>
                <TableCell className="text-center"><StatusBadge status={c.is_active ? "ACTIVE" : "INACTIVE"} /></TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </div>
  );

  return (
    <div className="p-6 space-y-4">
      <div>
        <Link href="/projects" className="text-xs text-muted-foreground hover:text-foreground">← 案件一覧</Link>
        <div className="flex items-center gap-3 mt-1">
          <h1 className="text-xl font-semibold text-foreground">{project.name}</h1>
          <StatusBadge status={project.is_active ? "ACTIVE" : "INACTIVE"} />
        </div>
        <p className="text-xs text-muted-foreground mt-0.5">{project.project_id} ／ {project.client_name}</p>
      </div>

      <div className="flex gap-1 border-b border-border overflow-x-auto">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            className={`px-4 py-2 text-sm whitespace-nowrap border-b-2 transition-colors ${
              tab === t.key ? "border-primary text-foreground font-medium" : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="bg-card border border-border rounded-lg p-4">
        {tab === "info" && (
          <div className="space-y-4">
            <dl className="grid grid-cols-2 gap-4 text-sm">
              <div><dt className="text-xs text-muted-foreground">クライアント</dt><dd>{project.client_name}</dd></div>
              <div><dt className="text-xs text-muted-foreground">EDI案件別名</dt><dd>{project.edi_project_alias || "—"}</dd></div>
              <div><dt className="text-xs text-muted-foreground">稼働報告の提出期限</dt>
                <dd>{project.report_deadline_type === "FIXED_DAY" ? `当月 ${project.report_deadline_value ?? "—"} 日` : `月末の ${project.report_deadline_value ?? "—"} 営業日前`}</dd></div>
              <div><dt className="text-xs text-muted-foreground">受注契約 / 発注契約</dt><dd>{clientContracts.length}件 / {partnerContracts.length}件</dd></div>
            </dl>
            <Button variant="outline" size="sm" className="border-border" onClick={() => setEditOpen(true)}>✏️ 案件情報を編集</Button>
          </div>
        )}

        {tab === "client" && contractTable(clientContracts, setClientContractId, `/client-contracts?project_id=${encodeURIComponent(projectId)}`, false)}
        {tab === "partner" && contractTable(partnerContracts, setPartnerContractId, `/partner-contracts?project_id=${encodeURIComponent(projectId)}`, true)}

        {tab === "received" && (
          <div className="space-y-3">
            <p className="text-xs text-muted-foreground">注文書は月次の業務で作成します。ここでは、この案件の注文書の履歴を確認できます。</p>
            {receivedOrders.length === 0 ? <Empty text="注文書はまだありません" /> : (
              <Table>
                <TableHeader><TableRow>
                  <TableHead>番号</TableHead><TableHead>対象月</TableHead><TableHead>技術者</TableHead><TableHead className="text-center">状態</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {receivedOrders.map((r) => (
                    <TableRow key={r.id}>
                      <TableCell className="text-sm"><Link href={`/received-orders/${r.id}`} className="text-emerald-400 hover:underline">{r.received_order_no}</Link></TableCell>
                      <TableCell className="text-sm text-muted-foreground">{r.target_month?.slice(0, 7)}</TableCell>
                      <TableCell className="text-sm">{r.engineer_name}</TableCell>
                      <TableCell className="text-center"><StatusBadge status={r.status} /></TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>
        )}

        {tab === "orders" && (
          <div className="space-y-3">
            <p className="text-xs text-muted-foreground">発注書は月次の業務で作成します。ここでは、この案件の発注書の履歴を確認できます。</p>
            {orders.length === 0 ? <Empty text="発注書はまだありません" /> : (
              <Table>
                <TableHeader><TableRow>
                  <TableHead>番号</TableHead><TableHead>パートナー</TableHead><TableHead>期間</TableHead><TableHead className="text-center">状態</TableHead>
                </TableRow></TableHeader>
                <TableBody>
                  {orders.map((o) => (
                    <TableRow key={o.order_id}>
                      <TableCell className="text-sm"><Link href={`/orders/${encodeURIComponent(o.order_id)}`} className="text-emerald-400 hover:underline">{o.order_id}</Link></TableCell>
                      <TableCell className="text-sm">{o.partner_name}</TableCell>
                      <TableCell className="text-sm text-muted-foreground">{o.work_start} 〜 {o.work_end}</TableCell>
                      <TableCell className="text-center"><StatusBadge status={o.status} /></TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>
        )}
      </div>

      <ProjectEditModal projectId={projectId} open={editOpen} onClose={() => setEditOpen(false)} />
      <ClientContractEditModal contractId={clientContractId} open={clientContractId != null} onClose={() => { setClientContractId(null); }} />
      <PartnerContractEditModal contractId={partnerContractId} open={partnerContractId != null} onClose={() => { setPartnerContractId(null); }} />
    </div>
  );
}
