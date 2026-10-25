"use client";

import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { useCurrentMonth } from "@/lib/MonthContext";
import { fetchStatusMatrix } from "@/lib/api";
import { NEXT_ACTION, TONE_CLASS } from "@/lib/status-matrix";
import type { StatusCell } from "@/lib/types";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { PageHeader } from "@/components/ui/page-header";

function CellBadge({ cell }: { cell: StatusCell }) {
  return (
    <span className={`inline-block text-[11px] px-2 py-px rounded-full border whitespace-nowrap ${TONE_CLASS[cell.tone]}`}>
      {cell.label}
    </span>
  );
}

/** 月別状況(サイドバー > 月次 > 月の締め > 月別状況)。アサインごとの進み具合と、いまできる次の操作。見るだけ・移動するだけ */
export default function StatusPage() {
  const { month } = useCurrentMonth();
  const [query, setQuery] = useState("");
  const [client, setClient] = useState("");
  const [onlyTodo, setOnlyTodo] = useState(true);

  const { data, isLoading, error } = useQuery({
    queryKey: ["status-matrix", month],
    queryFn: () => fetchStatusMatrix(month),
  });

  const rows = useMemo(() => data?.rows ?? [], [data]);
  const clients = useMemo(() => [...new Set(rows.map((r) => r.client_name))].sort((a, b) => a.localeCompare(b, "ja")), [rows]);
  const shown = rows.filter(
    (r) =>
      (!onlyTodo || r.next_action) &&
      (!client || r.client_name === client) &&
      (!query.trim() || `${r.engineer_name}${r.client_name}${r.project_name}`.includes(query.trim())),
  );
  const s = data?.summary;

  return (
    <div className="space-y-4 p-6">
      <PageHeader
        start={
        <div className="flex flex-wrap items-center gap-3">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="要員・取引先・案件で検索"
            className="px-3 py-1.5 text-sm bg-muted border border-border rounded-md text-foreground"
          />
          <select
            aria-label="取引先"
            value={client}
            onChange={(e) => setClient(e.target.value)}
            className="px-3 py-1.5 text-sm bg-muted border border-border rounded-md text-foreground"
          >
            <option value="">取引先: すべて</option>
            {clients.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
          <label className="flex items-center gap-1.5 text-sm text-muted-foreground">
            <input type="checkbox" checked={onlyTodo} onChange={(e) => setOnlyTodo(e.target.checked)} />
            要対応のみ
          </label>
        </div>
        }
      />

      {s && (
        <div className="flex flex-wrap gap-2" aria-label="集計">
          {[
            ["アサイン", s.total],
            ["要対応", s.needs_action],
            ["請求書 未確定", s.invoice_unconfirmed],
            ["勤務表 未提出・差し戻し", s.timesheet_stalled],
          ].map(([label, n]) => (
            <div key={label as string} className="rounded-lg border border-border bg-card px-3 py-2 text-xs text-muted-foreground">
              {label}
              <b className="block text-lg text-foreground">{n}</b>
            </div>
          ))}
        </div>
      )}

      {isLoading && <p className="text-sm text-muted-foreground">読み込み中...</p>}
      {error && <p className="text-sm text-red-400">月別状況を取得できませんでした。画面を更新してください。</p>}

      {data && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>要員</TableHead>
              <TableHead>取引先 / 案件</TableHead>
              <TableHead>受注書</TableHead>
              <TableHead>発注書</TableHead>
              <TableHead>勤務表</TableHead>
              <TableHead>請求書</TableHead>
              <TableHead>支払通知</TableHead>
              <TableHead>次の操作</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.length === 0 && (
              <TableRow>
                <TableCell colSpan={8} className="py-6 text-center text-sm text-muted-foreground">
                  該当するアサインがありません
                </TableCell>
              </TableRow>
            )}
            {shown.map((r) => {
              const act = r.next_action ? NEXT_ACTION[r.next_action] : null;
              return (
                <TableRow key={`${r.engineer_id}-${r.project_id}`}>
                  <TableCell className="text-sm">{r.engineer_name}</TableCell>
                  <TableCell className="text-sm">
                    {r.client_name}
                    <div className="text-xs text-muted-foreground">{r.project_name}</div>
                  </TableCell>
                  <TableCell><CellBadge cell={r.received_order} /></TableCell>
                  <TableCell><CellBadge cell={r.purchase_order} /></TableCell>
                  <TableCell><CellBadge cell={r.timesheet} /></TableCell>
                  <TableCell><CellBadge cell={r.invoice} /></TableCell>
                  <TableCell><CellBadge cell={r.notice} /></TableCell>
                  <TableCell>
                    {act ? (
                      <Link href={act.href} className="inline-block text-xs font-bold px-2.5 py-1 rounded-md bg-primary text-primary-foreground hover:opacity-90">
                        {act.label}
                      </Link>
                    ) : (
                      <span className="text-xs text-muted-foreground">対応なし</span>
                    )}
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}
    </div>
  );
}
