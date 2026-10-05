import { describe, expect, it } from "vitest";
import { getStatus, getAllStatuses } from "@/lib/status";

describe("getStatus", () => {
  it("returns the Japanese label and class for known statuses", () => {
    expect(getStatus("order", "SENT")).toEqual({
      label: "送付済",
      className: "border-sky-500/30 text-sky-400",
    });
  });

  // 請求書承認・送信ワークフロー（migrations/021）のステータスがフロントに
  // 反映されておらず、バッジが生の英語ステータスコードのまま表示される
  // バグを回帰させないためのテスト。
  it("includes the invoice approval workflow statuses", () => {
    expect(getStatus("invoice", "PENDING_APPROVAL").label).toBe("承認待ち");
    expect(getStatus("invoice", "APPROVED").label).toBe("承認済");
    expect(getStatus("invoice", "SENT").label).toBe("送付済");
  });

  it("falls back to the generic map when the domain lacks the status", () => {
    expect(getStatus("invoice", "ACTIVE")).toEqual({
      label: "有効",
      className: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
    });
  });

  it("falls back to the raw status code when nothing matches", () => {
    const result = getStatus("invoice", "SOMETHING_UNKNOWN");
    expect(result.label).toBe("SOMETHING_UNKNOWN");
  });

  it("falls back gracefully for an unknown domain", () => {
    const result = getStatus("not_a_domain", "DRAFT");
    expect(result.label).toBe("DRAFT");
  });
});

describe("getAllStatuses", () => {
  it("lists every status defined for a domain", () => {
    const statuses = getAllStatuses("timesheet").map((s) => s.value);
    expect(statuses).toEqual(
      expect.arrayContaining(["PENDING", "UPLOADED", "PARSED", "APPROVED", "REJECTED", "SENT"])
    );
  });

  it("returns an empty list for an unknown domain", () => {
    expect(getAllStatuses("not_a_domain")).toEqual([]);
  });
});

describe("status unification requirements", () => {
  // ①全ドメインの全コードが空でない label を持つ
  it("all statuses across all domains have non-empty labels", () => {
    const domains = ["order", "receivedOrder", "invoice", "notice", "timesheet", "payroll", "expense", "task", "settlement", "generic"];
    for (const domain of domains) {
      const statuses = getAllStatuses(domain);
      for (const status of statuses) {
        expect(status.label).toBeTruthy();
        expect(status.label.trim().length).toBeGreaterThan(0);
      }
    }
  });

  // ②`SENT` の label が order / invoice / notice / timesheet で全て「送付済」
  it("SENT label is '送付済' across order, invoice, notice, and timesheet", () => {
    expect(getStatus("order", "SENT").label).toBe("送付済");
    expect(getStatus("invoice", "SENT").label).toBe("送付済");
    expect(getStatus("notice", "SENT").label).toBe("送付済");
    expect(getStatus("timesheet", "SENT").label).toBe("送付済");
  });

  // ③receivedOrder に INVOICE_SENT/INVOICE_CONFIRMED がある
  it("receivedOrder has INVOICE_SENT and INVOICE_CONFIRMED statuses", () => {
    expect(getStatus("receivedOrder", "INVOICE_SENT").label).toBe("請求送付");
    expect(getStatus("receivedOrder", "INVOICE_CONFIRMED").label).toBe("受諾");
  });

  // ④timesheet の REJECTED が「差し戻し」
  it("timesheet REJECTED label is '差し戻し'", () => {
    expect(getStatus("timesheet", "REJECTED").label).toBe("差し戻し");
  });
});
