"use client";

import { Suspense, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { fetchMembers, fetchClientContracts, fetchPartnerContracts } from "@/lib/api";
import { AddMemberModal } from "@/components/members/add-member-modal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Plus } from "lucide-react";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { getMemberClassification, calculateAssignmentCount, getProposalPartners } from "@/lib/member-utils";

export default function MembersPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <MembersPageContent />
    </Suspense>
  );
}

function MembersPageContent() {
  const router = useRouter();
  const { isAdmin } = useCurrentUser();
  const mounted = useMounted();
  const showAdmin = isAdmin && mounted;
  const [nameFilter, setNameFilter] = useState("");
  const [classificationFilter, setClassificationFilter] = useState("");

  const { data: members = [] } = useQuery({
    queryKey: ["members"],
    queryFn: fetchMembers,
    enabled: showAdmin,
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

  // 分類オプションの生成
  const classificationOptions = useMemo(() => {
    const hasEmployee = members.some((m) => getMemberClassification(m.affiliation_type) === "自社社員");
    const hasPartner = members.some((m) => getMemberClassification(m.affiliation_type) === "パートナー要員");
    const opts: { value: string; label: string }[] = [];
    if (hasEmployee) opts.push({ value: "EMPLOYEE", label: "自社社員" });
    if (hasPartner) opts.push({ value: "PARTNER", label: "パートナー要員" });
    return opts;
  }, [members]);

  // フィルタリング
  const filteredMembers = useMemo(() => {
    return members.filter((m) => {
      if (nameFilter && !m.name.includes(nameFilter)) return false;
      if (classificationFilter) {
        const classification = getMemberClassification(m.affiliation_type);
        if (classificationFilter === "EMPLOYEE" && classification !== "自社社員") return false;
        if (classificationFilter === "PARTNER" && classification !== "パートナー要員") return false;
      }
      return true;
    });
  }, [members, nameFilter, classificationFilter]);

  // 新規作成モーダル
  // 追加の入口。null=閉じている / 自社社員追加 / パートナー要員追加
  const [addKind, setAddKind] = useState<"EMPLOYEE" | "PARTNER" | null>(null);

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">要員</h1>
          <p className="text-sm text-muted-foreground">自社社員とパートナー要員の管理</p>
        </div>
        {showAdmin && (
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => setAddKind("EMPLOYEE")} className="gap-2">
              <Plus className="h-4 w-4" />
              自社社員追加
            </Button>
            <Button onClick={() => setAddKind("PARTNER")} className="gap-2">
              <Plus className="h-4 w-4" />
              パートナー要員追加
            </Button>
          </div>
        )}
      </div>

      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>
                <SearchableColumnHeader
                  label="氏名"
                  value={nameFilter}
                  onChange={setNameFilter}
                  options={members.map((m) => ({ value: m.name, label: m.name }))}
                />
              </TableHead>
              <TableHead>
                <SearchableColumnHeader
                  label="区分"
                  value={classificationFilter}
                  onChange={setClassificationFilter}
                  options={classificationOptions}
                />
              </TableHead>
              <TableHead>現在のアサイン数</TableHead>
              <TableHead>提案元パートナー</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {filteredMembers.map((member) => (
              <TableRow
                key={member.id}
                onClick={() => router.push(`/members/${member.id}`)}
                className="cursor-pointer hover:bg-muted/50"
              >
                <TableCell className="font-medium">{member.name}</TableCell>
                <TableCell>
                  <Badge variant="outline">
                    {getMemberClassification(member.affiliation_type)}
                  </Badge>
                </TableCell>
                <TableCell>
                  {calculateAssignmentCount(member.id, clientContracts)}件
                </TableCell>
                <TableCell>
                  {getMemberClassification(member.affiliation_type) === "パートナー要員"
                    ? getProposalPartners(member.name, partnerContracts)
                    : "—"}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {addKind && (
        <AddMemberModal key={addKind} open classification={addKind} onClose={() => setAddKind(null)} />
      )}
    </div>
  );
}
