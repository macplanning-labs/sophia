import { describe, it, expect } from "vitest";
import {
  TABLE_LABELS,
  buildHandOverSummary,
  parseOwnedConflict,
  tableLabel,
  type OwnedByOrg,
  type OwnedCount,
} from "../owner-transfer";

describe("TABLE_LABELS", () => {
  it("covers all 11 担当データの種類 with the agreed Japanese labels", () => {
    expect(TABLE_LABELS).toEqual({
      m_client: "顧客",
      m_project: "案件",
      m_client_contract: "契約(売上)",
      m_partner_contract: "契約(仕入)",
      t_purchase_order: "発注",
      t_payment_notice: "支払通知",
      t_received_order: "受注",
      t_billing_invoice: "請求書",
      t_monthly_timesheet: "稼働報告",
      t_monthly_task: "月次タスク",
      s_company_info: "自動取込の受け皿",
    });
    expect(Object.keys(TABLE_LABELS)).toHaveLength(11);
  });

  it("tableLabel falls back to the raw table name for unknown tables", () => {
    expect(tableLabel("m_client")).toBe("顧客");
    expect(tableLabel("t_unknown")).toBe("t_unknown");
  });
});

describe("buildHandOverSummary", () => {
  it("sums the total and keeps only non-zero rows in label order", () => {
    const owned: OwnedCount[] = [
      { table: "t_billing_invoice", count: 2 },
      { table: "m_client", count: 3 },
      { table: "m_project", count: 0 },
    ];
    const s = buildHandOverSummary(owned);
    expect(s.total).toBe(5);
    expect(s.rows).toEqual([
      { table: "m_client", label: "顧客", count: 3 },
      { table: "t_billing_invoice", label: "請求書", count: 2 },
    ]);
    expect(s.orgLines).toEqual([]);
  });

  it("builds per-org lines and labels an unset organization", () => {
    const byOrg: OwnedByOrg[] = [
      { org_unit_id: 1, org_name: "営業一課", count: 4 },
      { org_unit_id: null, org_name: null, count: 1 },
      { org_unit_id: 2, org_name: "開発", count: 0 },
    ];
    const s = buildHandOverSummary([{ table: "m_client", count: 5 }], byOrg);
    expect(s.total).toBe(5);
    expect(s.orgLines).toEqual([
      { label: "営業一課", count: 4 },
      { label: "組織未設定", count: 1 },
    ]);
  });

  it("returns a zero total for an empty conflict", () => {
    const s = buildHandOverSummary([]);
    expect(s).toEqual({ total: 0, rows: [], orgLines: [] });
  });
});

describe("parseOwnedConflict", () => {
  it("returns the owned counts for a 409 with owned data", () => {
    const body = {
      error: "この社員が担当のデータがあります",
      total: 3,
      owned: [{ table: "m_client", count: 3 }],
    };
    expect(parseOwnedConflict(409, body)).toEqual([{ table: "m_client", count: 3 }]);
  });

  it("returns null for any status other than 409", () => {
    const body = { owned: [{ table: "m_client", count: 3 }] };
    expect(parseOwnedConflict(200, body)).toBeNull();
    expect(parseOwnedConflict(400, body)).toBeNull();
    expect(parseOwnedConflict(500, body)).toBeNull();
  });

  it("returns null for a 409 whose body is not the owned-data shape", () => {
    expect(parseOwnedConflict(409, { error: "別の競合" })).toBeNull();
    expect(parseOwnedConflict(409, null)).toBeNull();
    expect(parseOwnedConflict(409, "text")).toBeNull();
  });

  it("drops malformed entries inside owned", () => {
    const body = {
      owned: [{ table: "m_client", count: 1 }, { table: 5, count: 1 }, null, { table: "m_project" }],
    };
    expect(parseOwnedConflict(409, body)).toEqual([{ table: "m_client", count: 1 }]);
  });
});
