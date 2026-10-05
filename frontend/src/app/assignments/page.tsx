"use client";

import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { GripVertical, Plus } from "lucide-react";
import {
  fetchAssignmentOverview, fetchMembers, fetchMastersData, previewAssignment, createAssignment,
} from "@/lib/api";
import { useCurrentUser } from "@/lib/useCurrentUser";
import { useMounted } from "@/lib/useMounted";
import { useDebounced } from "@/lib/useDebounced";
import { useDragLink } from "@/hooks/use-drag-link";
import { Button } from "@/components/ui/button";
import { LinkPickerMenu } from "@/components/ui/link-picker-menu";
import { AssignmentPanel } from "@/components/assignments/assignment-panel";
import { AddMemberModal } from "@/components/members/add-member-modal";
import { getMemberClassification } from "@/lib/member-utils";
import { initialForm, previewKey, toCreateRequest, toPreviewRequest, type AssignmentForm } from "@/lib/assignment-form";
import { formatYen, formatPct, flowLabel, targetBadge } from "@/lib/assignment-display";
import type { AssignmentProject, MemberRow } from "@/lib/types";

const toneClass = {
  ok: "border-emerald-500/40 text-emerald-400",
  warn: "border-amber-500/40 text-amber-400",
  muted: "border-border text-muted-foreground",
} as const;

export default function AssignmentsPage() {
  const queryClient = useQueryClient();
  const { isAdmin } = useCurrentUser();
  const mounted = useMounted();
  const enabled = isAdmin && mounted;

  const [form, setForm] = useState<AssignmentForm | null>(null);
  const [filter, setFilter] = useState<"" | "EMPLOYEE" | "PARTNER">("");
  const [search, setSearch] = useState("");
  const [addKind, setAddKind] = useState<"EMPLOYEE" | "PARTNER" | null>(null);

  const { data: overview } = useQuery({ queryKey: ["assignment-overview"], queryFn: fetchAssignmentOverview, enabled });
  const { data: members = [] } = useQuery({ queryKey: ["members"], queryFn: fetchMembers, enabled });
  const { data: partners = [] } = useQuery<{ partner_id: string; name: string }[]>({
    queryKey: ["masters-data", "partners"],
    queryFn: () => fetchMastersData("partners"),
    enabled,
  });

  const projects = useMemo(() => overview?.projects ?? [], [overview]);
  const projectById = useMemo(() => new Map(projects.map((p) => [p.project_id, p])), [projects]);
  const activeMembers = useMemo(() => members.filter((m) => m.is_active), [members]);
  const memberById = useMemo(() => new Map(activeMembers.map((m) => [String(m.id), m])), [activeMembers]);

  const startAssignment = (engineerId: string, projectId: string) => {
    const m = memberById.get(engineerId);
    if (!m || !projectById.has(projectId)) return;
    setForm(initialForm({ id: m.id, name: m.name, affiliation_type: m.affiliation_type, partner_id: m.partner_id }, projectId, new Date()));
  };

  const { dragProps, dropProps, draggingId, overTargetId } = useDragLink({ onLink: startAssignment });

  // 利益の試算(入力が止まって0.3秒後に、サーバーへ問い合わせる)
  const key = useDebounced(form ? previewKey(form) : null, 300);
  const previewQuery = useQuery({
    queryKey: ["assignment-preview", key],
    queryFn: () => previewAssignment(JSON.parse(key!)),
    enabled: !!key && !!form,
    placeholderData: (prev) => prev,
    retry: false,
  });
  const preview = form && toPreviewRequest(form) ? previewQuery.data : undefined;

  const createMutation = useMutation({
    mutationFn: () => createAssignment(toCreateRequest(form!)),
    onSuccess: () => {
      toast.success("アサインを作成しました");
      queryClient.invalidateQueries({ queryKey: ["assignment-overview"] });
      queryClient.invalidateQueries({ queryKey: ["members"] });
      queryClient.invalidateQueries({ queryKey: ["clientContracts"] });
      queryClient.invalidateQueries({ queryKey: ["partnerContracts"] });
      setForm(null);
    },
    onError: (err: Error) => toast.error(err.message || "アサインを作成できませんでした"),
  });

  const grouped = useMemo(() => {
    const g = new Map<string, { clientName: string; projects: AssignmentProject[] }>();
    for (const p of projects) {
      const e = g.get(String(p.client_id)) ?? { clientName: p.client_name, projects: [] };
      e.projects.push(p);
      g.set(String(p.client_id), e);
    }
    return [...g.values()];
  }, [projects]);

  const projectOptions = useMemo(
    () => projects.map((p) => ({ value: p.project_id, label: p.project_name, group: p.client_name })),
    [projects],
  );

  const memberOptions = useMemo(
    () => activeMembers.map((m) => ({ value: String(m.id), label: m.name, group: getMemberClassification(m.affiliation_type) })),
    [activeMembers],
  );

  const visibleMembers = activeMembers.filter((m) => {
    if (filter && getMemberClassification(m.affiliation_type) !== (filter === "EMPLOYEE" ? "自社社員" : "パートナー要員")) return false;
    return !search.trim() || m.name.includes(search.trim());
  });

  const engineer = form ? memberById.get(String(form.engineer_id)) : undefined;
  const targetProject = form ? projectById.get(form.project_id) : undefined;

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between gap-4">
        <ol className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm" aria-label="使い方">
          <li className="flex items-center gap-1.5"><span className="flex h-5 w-5 items-center justify-center rounded-full bg-primary/20 text-xs font-semibold text-primary">1</span>右の要員をつかんで</li>
          <li aria-hidden className="text-muted-foreground">→</li>
          <li className="flex items-center gap-1.5"><span className="flex h-5 w-5 items-center justify-center rounded-full bg-primary/20 text-xs font-semibold text-primary">2</span>左の案件カードへドラッグ</li>
          <li aria-hidden className="text-muted-foreground">→</li>
          <li className="flex items-center gap-1.5"><span className="flex h-5 w-5 items-center justify-center rounded-full bg-primary/20 text-xs font-semibold text-primary">3</span>開いたパネルで条件を入れて、利益を確認し、アサイン作成</li>
          <li className="basis-full text-xs text-muted-foreground">ドラッグが難しいときは、要員の「案件を選ぶ」、または案件カードの「要員を選ぶ」から選べます。</li>
        </ol>
        {enabled && (
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => setAddKind("EMPLOYEE")}><Plus className="h-4 w-4" />自社社員追加</Button>
            <Button variant="outline" size="sm" onClick={() => setAddKind("PARTNER")}><Plus className="h-4 w-4" />パートナー要員追加</Button>
          </div>
        )}
      </div>

      <div className={`grid gap-6 lg:grid-cols-[1fr_340px] ${form ? "lg:pr-[480px]" : ""}`}>
        {/* 左: 取引先ごとの案件カード */}
        <section aria-label="案件" className="space-y-5">
          {grouped.length === 0 && <p className="text-sm text-muted-foreground">アサインできる案件がありません。</p>}
          {grouped.map((g) => (
            <div key={g.clientName} className="space-y-2">
              <h2 className="text-sm font-semibold text-muted-foreground">{g.clientName}</h2>
              <div className="grid gap-3 md:grid-cols-2">
                {g.projects.map((p) => {
                  const t = targetBadge(p.status, p.diff_pct, p.target_pct);
                  const isTarget = form?.project_id === p.project_id;
                  const after = isTarget && preview ? preview.project_total.after : null;
                  return (
                    <div
                      key={p.project_id}
                      data-testid={`project-${p.project_id}`}
                      {...dropProps(p.project_id)}
                      className={`rounded-lg border bg-card p-3 transition-colors ${
                        overTargetId === p.project_id
                          ? "border-primary bg-primary/10"
                          : draggingId
                            ? "border-dashed border-primary/60"
                            : isTarget ? "border-primary/60" : "border-border"
                      }`}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <p className="font-medium text-sm">{p.project_name}</p>
                        <span className="shrink-0 rounded border border-border px-1.5 py-0.5 text-[11px] text-muted-foreground">{flowLabel(p.commercial_flow)}</span>
                      </div>
                      <p className="mt-1 text-xs text-muted-foreground">
                        アサイン {p.assignment_count}名{p.internal_count > 0 ? `(うち自社社員 ${p.internal_count}名。原価は含まず)` : ""}
                      </p>
                      <div className="mt-2 flex items-baseline justify-between text-sm">
                        <span>月額売上 {formatYen(p.revenue)}</span>
                        <span className={p.gross_profit < 0 ? "font-semibold text-red-400" : "font-semibold"}>
                          粗利 {formatYen(p.gross_profit)}({formatPct(p.margin_pct)})
                        </span>
                      </div>
                      <span className={`mt-2 inline-block rounded border px-2 py-0.5 text-[11px] ${toneClass[t.tone]}`}>{t.text}</span>
                      {after && (
                        <p className="mt-2 rounded bg-primary/10 px-2 py-1 text-xs">
                          この要員を加えると: 粗利 {formatYen(after.gross_profit)}({formatPct(after.margin_pct)})
                        </p>
                      )}
                      {draggingId ? (
                        <p className="mt-2 text-center text-xs font-medium text-primary">
                          {overTargetId === p.project_id ? "離すとアサイン作成パネルが開きます" : "ここにドロップ"}
                        </p>
                      ) : (
                        <div className="mt-2">
                          <LinkPickerMenu
                            label={`${p.project_name}に要員をアサイン`}
                            triggerText="要員を選ぶ"
                            options={memberOptions}
                            onSelect={(memberId) => startAssignment(memberId, p.project_id)}
                            emptyText="要員がいません"
                          />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </section>

        {/* 右: 要員 */}
        <section aria-label="要員" className="space-y-3">
          <div className="flex gap-2">
            <input
              className="min-w-0 flex-1 rounded-md border border-border bg-muted px-3 py-1.5 text-sm"
              placeholder="氏名で検索"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              aria-label="要員の検索"
            />
            <select
              className="rounded-md border border-border bg-muted px-2 py-1.5 text-sm"
              value={filter}
              onChange={(e) => setFilter(e.target.value as "" | "EMPLOYEE" | "PARTNER")}
              aria-label="区分"
            >
              <option value="">すべて</option>
              <option value="EMPLOYEE">自社社員</option>
              <option value="PARTNER">パートナー要員</option>
            </select>
          </div>
          <p className="text-xs text-muted-foreground">要員をつかんで、左の案件カードへドラッグします。</p>
          <ul className="space-y-2">
            {visibleMembers.map((m: MemberRow) => (
              <li
                key={m.id}
                {...dragProps(String(m.id))}
                aria-label={`${m.name}(ドラッグで案件へ)`}
                className={`flex items-center justify-between rounded-lg border bg-card px-3 py-2 text-sm cursor-grab ${draggingId === String(m.id) ? "opacity-50" : ""} ${form?.engineer_id === m.id ? "border-primary/60" : "border-border"}`}
              >
                <span className="flex items-center gap-2">
                  <GripVertical className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden />
                  <span>
                    {m.name}
                    <span className="ml-2 rounded border border-border px-1.5 py-0.5 text-[11px] text-muted-foreground">{getMemberClassification(m.affiliation_type)}</span>
                  </span>
                </span>
                <LinkPickerMenu
                  label={`${m.name}を案件へ編成`}
                  triggerText="案件を選ぶ"
                  options={projectOptions}
                  onSelect={(projectId) => startAssignment(String(m.id), projectId)}
                  emptyText="案件がありません"
                />
              </li>
            ))}
            {visibleMembers.length === 0 && <li className="text-sm text-muted-foreground">該当する要員がいません。</li>}
          </ul>
        </section>
      </div>

      {form && engineer && targetProject && (
        <AssignmentPanel
          form={form}
          setForm={setForm}
          engineerName={engineer.name}
          projectLabel={`${targetProject.client_name} / ${targetProject.project_name}`}
          partners={partners}
          preview={preview}
          previewLoading={previewQuery.isFetching}
          previewError={previewQuery.error ? (previewQuery.error as Error).message : null}
          creating={createMutation.isPending}
          onCreate={() => createMutation.mutate()}
          onClose={() => setForm(null)}
        />
      )}

      {addKind && <AddMemberModal key={addKind} open classification={addKind} onClose={() => setAddKind(null)} />}
    </div>
  );
}
