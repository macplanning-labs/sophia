// lib/useMasterData.ts — マスタデータ取得カスタムフック
//
// ■ 使い方:
//   const { data: engineers, isLoading } = useMasterData("engineers");
//   const { data: clients } = useMasterData("clients", { enabled: showModal });
//
// ■ マスタキー一覧（routes.rs の MASTER_TABLES 定義と対応）:
//   キー名              テーブル名            備考
//   ─────────────────────────────────────────────────────
//   clients             m_client              クライアント
//   partners            m_partner             パートナー
//   projects            m_project             案件
//   engineers           m_engineer            技術者
//   partner_contracts   m_partner_contract    発注契約
//   client_contracts    m_client_contract     顧客契約
//   payment_terms       m_payment_term        支払条件
//   contract_progress   m_contract_progress   基本契約進捗
//   bank_masters        m_bank_master         銀行マスタ
//   insurance_rates     m_insurance_rate      社保料率
//   withholding_taxes   m_withholding_tax     源泉徴収
//   employees           m_employee            社員
//   email_templates     s_email_template      メールテンプレート
//   ※ workplaces / work_locations は汎用マスタから削除済み（案件・契約側で入力）
//
// ■ レスポンス形式:
//   GET /api/v1/masters/{key} → MasterRow[] (配列を直接返却。{ rows: [...] } ではない)
//
// ■ ルートグループ:
//   /api/v1/masters/* は staff_routes 配下（Admin/Staff のみアクセス可）
//   Portal（パートナー/エンジニア）からはアクセス不可
//
// ■ 注意:
//   - テーブル名（m_engineer）ではなくキー名（engineers）を使うこと
//   - 新しいマスタを追加したら、上記の一覧を更新すること

import { useQuery, type UseQueryOptions } from "@tanstack/react-query";

export interface MasterRow {
  id: number;
  name?: string;
  [key: string]: unknown;
}

/**
 * マスタデータ取得フック
 *
 * @param masterKey - マスタキー名（例: "engineers", "clients"）
 *                    ※ テーブル名（m_engineer）ではなくキー名を使う
 * @param options - useQuery のオプション（enabled, staleTime など）
 * @returns useQuery の戻り値。data は MasterRow[] 型
 */
export function useMasterData(
  masterKey: string,
  options?: { enabled?: boolean; staleTime?: number }
) {
  return useQuery<MasterRow[], Error, MasterRow[]>({
    queryKey: ["master", masterKey],
    queryFn: async (): Promise<MasterRow[]> => {
      const res = await fetch(`/api/v1/masters/${masterKey}`, {
        credentials: "include",
      });
      if (!res.ok) {
        throw new Error(`マスタ取得失敗: ${masterKey} (${res.status})`);
      }
      const data = await res.json();
      // マスタAPIは配列を直接返す（{ rows: [...] } ではない）
      if (!Array.isArray(data)) {
        console.error(`[useMasterData] 想定外のレスポンス形式: ${masterKey}`, data);
        return [];
      }
      return data as MasterRow[];
    },
    staleTime: options?.staleTime ?? 60_000, // デフォルト1分キャッシュ
    enabled: options?.enabled,
    initialData: undefined,
  });
}
