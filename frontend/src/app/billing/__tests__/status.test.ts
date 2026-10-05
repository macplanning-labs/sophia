import { describe, it, expect } from "vitest";
import type { BillingInvoicePreview } from "@/lib/types";

// Helper function to get status display (extracted from page)
function getInvoiceStatus(invoice: BillingInvoicePreview): string {
  if (invoice.edi_excluded) {
    return "EDI連携のため対象外";
  }
  if (invoice.confirmable) {
    return "確定できます";
  }
  if (invoice.items.length === 0) {
    const suffix = invoice.already_issued_count > 0
      ? ` (発行済 ${invoice.already_issued_count}名)`
      : "";
    return `確定する明細がありません${suffix}`;
  }
  // Has missing items
  return `あと${invoice.missing.length}名`;
}

describe("getInvoiceStatus", () => {
  it("should return 'EDI連携のため対象外' when EDI excluded", () => {
    const invoice: BillingInvoicePreview = {
      key: "c1",
      client_id: 1,
      client_name: "取引先1",
      billing_unit: "PROJECT",
      project_id: "PRJ001",
      subject: "案件1 2026年10月分 請求書",
      items: [{ client_contract_id: 1, engineer_id: 1, engineer_name: "要員1", project_id: "PRJ001", project_name: "案件1", timesheet_id: 1, amount: 100000 }],
      subtotal: 100000,
      tax_amount: 10000,
      total: 110000,
      missing: [],
      already_issued_count: 0,
      confirmable: true,
      edi_excluded: true,
    };

    expect(getInvoiceStatus(invoice)).toBe("EDI連携のため対象外");
  });

  it("should return '確定できます' when confirmable", () => {
    const invoice: BillingInvoicePreview = {
      key: "c1",
      client_id: 1,
      client_name: "取引先1",
      billing_unit: "PROJECT",
      project_id: "PRJ001",
      subject: "案件1 2026年10月分 請求書",
      items: [{ client_contract_id: 1, engineer_id: 1, engineer_name: "要員1", project_id: "PRJ001", project_name: "案件1", timesheet_id: 1, amount: 100000 }],
      subtotal: 100000,
      tax_amount: 10000,
      total: 110000,
      missing: [],
      already_issued_count: 0,
      confirmable: true,
      edi_excluded: false,
    };

    expect(getInvoiceStatus(invoice)).toBe("確定できます");
  });

  it("should return '確定する明細がありません' when no items", () => {
    const invoice: BillingInvoicePreview = {
      key: "c1",
      client_id: 1,
      client_name: "取引先1",
      billing_unit: "PROJECT",
      project_id: "PRJ001",
      subject: "案件1 2026年10月分 請求書",
      items: [],
      subtotal: 0,
      tax_amount: 0,
      total: 0,
      missing: [],
      already_issued_count: 0,
      confirmable: false,
      edi_excluded: false,
    };

    expect(getInvoiceStatus(invoice)).toBe("確定する明細がありません");
  });

  it("should include issued count when no items but already issued", () => {
    const invoice: BillingInvoicePreview = {
      key: "c1",
      client_id: 1,
      client_name: "取引先1",
      billing_unit: "PROJECT",
      project_id: "PRJ001",
      subject: "案件1 2026年10月分 請求書",
      items: [],
      subtotal: 0,
      tax_amount: 0,
      total: 0,
      missing: [],
      already_issued_count: 2,
      confirmable: false,
      edi_excluded: false,
    };

    expect(getInvoiceStatus(invoice)).toBe("確定する明細がありません (発行済 2名)");
  });

  it("should return count of missing when has missing items", () => {
    const invoice: BillingInvoicePreview = {
      key: "c1",
      client_id: 1,
      client_name: "取引先1",
      billing_unit: "PROJECT",
      project_id: "PRJ001",
      subject: "案件1 2026年10月分 請求書",
      items: [{ client_contract_id: 1, engineer_id: 1, engineer_name: "要員1", project_id: "PRJ001", project_name: "案件1", timesheet_id: 1, amount: 100000 }],
      subtotal: 100000,
      tax_amount: 10000,
      total: 110000,
      missing: [
        { client_contract_id: 2, engineer_name: "要員2", project_id: "PRJ001", project_name: "案件1", timesheet_status: "UPLOADED" },
        { client_contract_id: 3, engineer_name: "要員3", project_id: "PRJ001", project_name: "案件1", timesheet_status: "PENDING" },
      ],
      already_issued_count: 0,
      confirmable: false,
      edi_excluded: false,
    };

    expect(getInvoiceStatus(invoice)).toBe("あと2名");
  });
});

describe("formatPrice", () => {
  function formatPrice(amount: number): string {
    return `¥${amount.toLocaleString("ja-JP")}`;
  }

  it("should format price with thousand separators", () => {
    expect(formatPrice(1000)).toBe("¥1,000");
    expect(formatPrice(100000)).toBe("¥100,000");
    expect(formatPrice(1234567)).toBe("¥1,234,567");
  });

  it("should format small amounts", () => {
    expect(formatPrice(0)).toBe("¥0");
    expect(formatPrice(100)).toBe("¥100");
    expect(formatPrice(999)).toBe("¥999");
  });
});
