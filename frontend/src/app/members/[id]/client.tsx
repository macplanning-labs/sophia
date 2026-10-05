"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchMemberDetail, fetchClientContracts, fetchPartnerContracts } from "@/lib/api";
import { useDynamicId } from "@/lib/utils";
import { DetailLayout, Field, FieldGrid } from "@/components/detail-layout";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { getMemberClassification } from "@/lib/member-utils";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";

interface Props {
  /** When opened from list modal; falls back to useDynamicId() */
  id?: string;
  embedded?: boolean;
  onDeleted?: () => void;
}

export default function MemberDetailPage({ id: idProp, embedded = false }: Props = {}) {
  const dynamicId = useDynamicId();
  const id = idProp || dynamicId;
  const { isAdmin } = useCurrentUser();
  const mounted = useMounted();
  const showAdmin = isAdmin && mounted;

  const { data, isLoading } = useQuery({
    queryKey: ["members", id],
    queryFn: () => fetchMemberDetail(id),
    enabled: !!id,
  });

  const { data: clientContracts = [] } = useQuery({
    queryKey: ["clientContracts"],
    queryFn: fetchClientContracts,
    enabled: showAdmin,
  });

  const { data: partnerContracts = [] } = useQuery({
    queryKey: ["partnerContracts"],
    queryFn: fetchPartnerContracts,
    enabled: showAdmin,
  });

  const assignmentHistory = data
    ? clientContracts
        .filter((c) => c.engineer_id === parseInt(id, 10))
        .map((c) => ({
          clientContractId: c.id,
          projectName: c.project_name,
          startDate: c.start_date,
          endDate: c.end_date,
          unitPrice: c.base_rate,
          partnerContracts: partnerContracts
            .filter(
              (p) =>
                p.engineer_name === data.name &&
                p.start_date === c.start_date
            )
            .map((p) => ({
              id: p.id,
              partnerName: p.partner_name,
              unitPrice: p.base_rate,
              startDate: p.start_date,
              endDate: p.end_date,
            })),
        }))
    : [];

  return (
    <DetailLayout
      title={data ? `要員 #${id}` : "読み込み中..."}
      icon="👤"
      backHref="/members"
      backLabel="一覧に戻る"
      isLoading={isLoading}
      embedded={embedded}
    >
      {data && (
        <>
          <FieldGrid>
            <Field label="氏名" value={data.name} />
            <Field label="カナ" value={data.name_kana || "—"} />
            <Field
              label="区分"
              value={
                <Badge variant="outline">
                  {getMemberClassification(data.affiliation_type)}
                </Badge>
              }
            />
            <Field label="メール" value={data.email || "—"} />
          </FieldGrid>

          {getMemberClassification(data.affiliation_type) === "パートナー要員" && (
            <FieldGrid>
              <Field
                label="パートナー"
                value={
                  data.partner_id
                    ? `${data.partner_id}`
                    : "—"
                }
              />
            </FieldGrid>
          )}

          {/* アサイン履歴 */}
          <div className="mt-8">
            <h2 className="text-lg font-semibold mb-4">アサイン履歴</h2>
            {assignmentHistory.length === 0 ? (
              <div className="text-sm text-muted-foreground p-4 text-center">
                アサイン履歴がありません
              </div>
            ) : (
              <div className="space-y-6">
                {assignmentHistory.map((history) => (
                  <div key={history.clientContractId} className="border rounded-lg p-4">
                    <div className="grid grid-cols-2 gap-4 mb-4">
                      <Field label="案件" value={history.projectName} />
                      <Field
                        label="期間"
                        value={`${history.startDate} ～ ${history.endDate}`}
                      />
                      <Field
                        label="受注単価"
                        value={`¥${history.unitPrice.toLocaleString()}`}
                      />
                    </div>

                    {/* 発注契約情報 */}
                    {history.partnerContracts.length > 0 && (
                      <div className="mt-4 pt-4 border-t">
                        <h3 className="text-sm font-medium mb-3">発注契約</h3>
                        <Table>
                          <TableHeader>
                            <TableRow>
                              <TableHead>パートナー名</TableHead>
                              <TableHead>期間</TableHead>
                              <TableHead>単価</TableHead>
                            </TableRow>
                          </TableHeader>
                          <TableBody>
                            {history.partnerContracts.map((pc) => (
                              <TableRow key={pc.id}>
                                <TableCell>{pc.partnerName}</TableCell>
                                <TableCell>
                                  {pc.startDate} ～ {pc.endDate}
                                </TableCell>
                                <TableCell>
                                  ¥{pc.unitPrice.toLocaleString()}
                                </TableCell>
                              </TableRow>
                            ))}
                          </TableBody>
                        </Table>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </DetailLayout>
  );
}
