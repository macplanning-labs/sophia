"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchProjects } from "@/lib/api";
import { StatusBadge } from "@/components/ui/status-badge";
import { Button } from "@/components/ui/button";
import {
  Table, TableBody, TableCell, TableHead, TableHeader, TableRow,
} from "@/components/ui/table";
import { SearchableColumnHeader } from "@/components/ui/searchable-column-header";
import { Plus } from "lucide-react";
import { ProjectEditModal } from "@/components/projects/ProjectEditModal";

export default function ProjectsPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <ProjectsPageContent />
    </Suspense>
  );
}

function ProjectsPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [editId, setEditId] = useState<string | null>(null);
  const [nameFilter, setNameFilter] = useState("");
  const [clientFilter, setClientFilter] = useState("");
  const [activeFilter, setActiveFilter] = useState("");

  const { data: projects, isLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: fetchProjects,
  });

  // /projects/[id] からのリダイレクト (?edit=) および直接リンクに対応
  useEffect(() => {
    const fromUrl = searchParams.get("edit");
    if (fromUrl) setEditId(fromUrl);
  }, [searchParams]);

  const openEdit = useCallback((projectId: string) => {
    setEditId(projectId);
    router.replace(`/projects?edit=${encodeURIComponent(projectId)}`, { scroll: false });
  }, [router]);

  const closeEdit = useCallback(() => {
    setEditId(null);
    router.replace("/projects", { scroll: false });
  }, [router]);

  const rows = projects ?? [];

  const nameOptions = useMemo(() => {
    return rows
      .map((p) => ({
        value: p.project_id,
        label: p.name,
        searchText: p.project_id,
      }))
      .sort((a, b) => a.label.localeCompare(b.label, "ja"));
  }, [rows]);

  const clientOptions = useMemo(() => {
    const names = new Set<string>();
    for (const p of rows) {
      if (p.client_name) names.add(p.client_name);
    }
    return [...names]
      .sort((a, b) => a.localeCompare(b, "ja"))
      .map((name) => ({ value: name, label: name }));
  }, [rows]);

  const activeOptions = useMemo(() => {
    const opts: { value: string; label: string }[] = [];
    if (rows.some((p) => p.is_active)) opts.push({ value: "active", label: "有効" });
    if (rows.some((p) => !p.is_active)) opts.push({ value: "inactive", label: "無効" });
    return opts;
  }, [rows]);

  const filteredProjects = useMemo(
    () =>
      rows.filter((p) => {
        if (nameFilter && p.project_id !== nameFilter) return false;
        if (clientFilter && p.client_name !== clientFilter) return false;
        if (activeFilter === "active" && !p.is_active) return false;
        if (activeFilter === "inactive" && p.is_active) return false;
        return true;
      }),
    [rows, nameFilter, clientFilter, activeFilter]
  );

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-end">
        <Button size="sm" className="bg-blue-600 hover:bg-blue-700 text-foreground gap-1" onClick={() => router.push("/projects/new")}>
          <Plus className="w-4 h-4" /> 新規作成
        </Button>
      </div>
      <div className="bg-card border border-border rounded-lg overflow-hidden">
        {isLoading ? (
          <div className="p-8 space-y-3">{[...Array(4)].map((_, i) => <div key={i} className="h-10 bg-muted/50 rounded animate-pulse" />)}</div>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="border-border hover:bg-transparent">
                <TableHead className="text-xs text-muted-foreground">案件ID</TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="案件名"
                    options={nameOptions}
                    value={nameFilter}
                    onChange={setNameFilter}
                    placeholder="案件名・IDで検索…"
                  />
                </TableHead>
                <TableHead className="text-xs">
                  <SearchableColumnHeader
                    label="クライアント"
                    options={clientOptions}
                    value={clientFilter}
                    onChange={setClientFilter}
                    placeholder="クライアント名で検索…"
                  />
                </TableHead>
                <TableHead className="text-xs text-muted-foreground">登録日</TableHead>
                <TableHead className="text-xs text-center">
                  <SearchableColumnHeader
                    label="状態"
                    options={activeOptions}
                    value={activeFilter}
                    onChange={setActiveFilter}
                    placeholder="状態で検索…"
                  />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.length === 0 ? (
                <TableRow><TableCell colSpan={5} className="text-center py-12 text-muted-foreground">データがありません</TableCell></TableRow>
              ) : filteredProjects.length === 0 ? (
                <TableRow><TableCell colSpan={5} className="text-center py-12 text-muted-foreground">条件に一致するデータがありません</TableCell></TableRow>
              ) : filteredProjects.map((p) => (
                <TableRow
                  key={p.project_id}
                  className="border-border/50 cursor-pointer hover:bg-accent/50"
                  onClick={() => openEdit(p.project_id)}
                >
                  <TableCell className="text-sm tabular-nums text-muted-foreground">{p.project_id}</TableCell>
                  <TableCell className="text-sm font-medium text-emerald-400">{p.name}</TableCell>
                  <TableCell className="text-sm text-muted-foreground">{p.client_name}</TableCell>
                  <TableCell className="text-[11px] text-muted-foreground">{p.created_at?.slice(0, 10)}</TableCell>
                  <TableCell className="text-center">
                    <StatusBadge status={p.is_active ? "ACTIVE" : "INACTIVE"} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        <div className="px-4 py-2 border-t border-border bg-background/60">
          <span className="text-xs text-muted-foreground">表示中: {filteredProjects.length}件</span>
        </div>
      </div>

      <ProjectEditModal
        projectId={editId}
        open={!!editId}
        onClose={closeEdit}
      />
    </div>
  );
}
