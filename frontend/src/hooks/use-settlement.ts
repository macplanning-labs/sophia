// hooks/use-settlement.ts — TanStack Query hooks（プリフェッチ + 楽観的UI）

"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  fetchSettlement,
  fetchFilters,
  issueInvoices,
  issueNotices,
} from "@/lib/api";
import type { SettlementApiResponse } from "@/lib/types";

// ── 月計算ヘルパー ──

function addMonths(dateStr: string, n: number): string {
  const d = new Date(dateStr);
  d.setMonth(d.getMonth() + n);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-01`;
}

// ── メインデータ取得 ──

export function useSettlement(params: {
  month?: string;
  client_id?: string;
  partner_id?: string;
  project_id?: string;
  status?: string;
}) {
  const qc = useQueryClient();
  const month = params.month;

  const query = useQuery({
    queryKey: ["settlement", params],
    queryFn: () => fetchSettlement(params),
    placeholderData: (prev) => prev, // 前のデータを保持してちらつき防止
  });

  // ── プリフェッチ: 前月・翌月 ──
  useEffect(() => {
    if (!month) return;
    const prev = addMonths(month, -1);
    const next = addMonths(month, 1);

    qc.prefetchQuery({
      queryKey: ["settlement", { ...params, month: prev }],
      queryFn: () => fetchSettlement({ ...params, month: prev }),
    });
    qc.prefetchQuery({
      queryKey: ["settlement", { ...params, month: next }],
      queryFn: () => fetchSettlement({ ...params, month: next }),
    });
  }, [month, qc, params]);

  return query;
}

// ── フィルタ用マスタデータ ──

export function useFilters() {
  return useQuery({
    queryKey: ["settlement-filters"],
    queryFn: fetchFilters,
    staleTime: 5 * 60 * 1000, // 5分キャッシュ（マスタデータは変わりにくい）
  });
}

// ── 請求書一括発行（楽観的UI） ──

export function useIssueInvoices(month: string) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (ids: number[]) => issueInvoices(ids, month),
    onMutate: async (ids) => {
      // 進行中のクエリをキャンセル
      await qc.cancelQueries({ queryKey: ["settlement"] });

      // 現在のデータをスナップショット
      const allQueries = qc.getQueriesData<SettlementApiResponse>({
        queryKey: ["settlement"],
      });

      // 楽観的更新: 対象行のinvoice_issuedをtrueに
      qc.setQueriesData<SettlementApiResponse>(
        { queryKey: ["settlement"] },
        (old) => {
          if (!old) return old;
          return {
            ...old,
            rows: old.rows.map((vr) =>
              ids.includes(vr.row.client_contract_id)
                ? { ...vr, row: { ...vr.row, invoice_issued: true } }
                : vr
            ),
            summary: {
              ...old.summary,
              invoice_issued_count:
                old.summary.invoice_issued_count +
                old.rows.filter(
                  (vr) =>
                    ids.includes(vr.row.client_contract_id) &&
                    !vr.row.invoice_issued
                ).length,
              invoice_pending_count: Math.max(
                0,
                old.summary.invoice_pending_count -
                  old.rows.filter(
                    (vr) =>
                      ids.includes(vr.row.client_contract_id) &&
                      !vr.row.invoice_issued
                  ).length
              ),
            },
          };
        }
      );

      return { allQueries };
    },
    onError: (_err, _ids, context) => {
      // ロールバック
      if (context?.allQueries) {
        for (const [key, data] of context.allQueries) {
          if (data) qc.setQueryData(key, data);
        }
      }
    },
    onSettled: () => {
      // サーバーと再同期
      qc.invalidateQueries({ queryKey: ["settlement"] });
    },
  });
}

// ── 支払通知書一括発行（楽観的UI） ──

export function useIssueNotices(month: string) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (ids: number[]) => issueNotices(ids, month),
    onMutate: async (ids) => {
      await qc.cancelQueries({ queryKey: ["settlement"] });

      const allQueries = qc.getQueriesData<SettlementApiResponse>({
        queryKey: ["settlement"],
      });

      qc.setQueriesData<SettlementApiResponse>(
        { queryKey: ["settlement"] },
        (old) => {
          if (!old) return old;
          const matches = (vr: (typeof old.rows)[number]) =>
            vr.row.partner_contract_id != null &&
            ids.includes(vr.row.partner_contract_id);
          return {
            ...old,
            rows: old.rows.map((vr) =>
              matches(vr)
                ? { ...vr, row: { ...vr.row, notice_issued: true } }
                : vr
            ),
            summary: {
              ...old.summary,
              notice_issued_count:
                old.summary.notice_issued_count +
                old.rows.filter(
                  (vr) => matches(vr) && !vr.row.notice_issued
                ).length,
              notice_pending_count: Math.max(
                0,
                old.summary.notice_pending_count -
                  old.rows.filter(
                    (vr) => matches(vr) && !vr.row.notice_issued
                  ).length
              ),
            },
          };
        }
      );

      return { allQueries };
    },
    onError: (_err, _ids, context) => {
      if (context?.allQueries) {
        for (const [key, data] of context.allQueries) {
          if (data) qc.setQueryData(key, data);
        }
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ["settlement"] });
    },
  });
}
