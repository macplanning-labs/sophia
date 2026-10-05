import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import {
  getMemberClassification,
  calculateAssignmentCount,
  getProposalPartners,
} from "../member-utils";
import type { ClientContractRow, ContractRow } from "../types";

describe("member-utils", () => {
  describe("getMemberClassification", () => {
    it("should return 自社社員 for EMPLOYEE type", () => {
      expect(getMemberClassification("EMPLOYEE")).toBe("自社社員");
    });

    it("should return 自社社員 for INTERNAL type", () => {
      expect(getMemberClassification("INTERNAL")).toBe("自社社員");
    });

    it("should return パートナー要員 for PARTNER type", () => {
      expect(getMemberClassification("PARTNER")).toBe("パートナー要員");
    });

    it("should return パートナー要員 for unknown type", () => {
      expect(getMemberClassification("UNKNOWN")).toBe("パートナー要員");
    });
  });

  describe("calculateAssignmentCount", () => {
    const mockContracts: ClientContractRow[] = [
      {
        id: 1,
        engineer_id: 1,
        project_id: "PRJ001",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        settlement_type: "RANGE",
        base_rate: 600000,
        effort: "160",
        is_active: true,
        project_name: "Project A",
        client_name: "Client A",
        engineer_name: "Engineer 1",
      },
      {
        id: 2,
        engineer_id: 1,
        project_id: "PRJ002",
        start_date: "2027-01-01",
        end_date: "2027-12-31",
        settlement_type: "RANGE",
        base_rate: 700000,
        effort: "160",
        is_active: true,
        project_name: "Project B",
        client_name: "Client B",
        engineer_name: "Engineer 1",
      },
      {
        id: 3,
        engineer_id: 2,
        project_id: "PRJ003",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        settlement_type: "RANGE",
        base_rate: 500000,
        effort: "160",
        is_active: true,
        project_name: "Project C",
        client_name: "Client C",
        engineer_name: "Engineer 2",
      },
    ];

    // 「今日」を 2026-10-04 に固定する。対象のコードは new Date() で今日を取るため、Date.now の差し替えでは効かず、
    // 実行日によって結果が変わっていた（2026-10-05 以降は失敗。DEMO-000152）。
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date("2026-10-04T03:00:00Z"));
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it("should count contracts where engineer_id matches and date is within range", () => {
      const count = calculateAssignmentCount(1, mockContracts);
      expect(count).toBe(1); // Only PRJ001 is within range
    });

    it("should return 0 for engineer with no active contracts", () => {
      const count = calculateAssignmentCount(99, mockContracts);
      expect(count).toBe(0);
    });

    it("should exclude contracts where today is outside the range", () => {
      // Engineer 1 has PRJ002 which starts 2027-01-01
      const count = calculateAssignmentCount(1, mockContracts);
      expect(count).toBe(1); // Only PRJ001 is active on 2026-10-04
    });

    it("should handle boundary dates correctly - start date inclusive", () => {
      const boundaryContracts: ClientContractRow[] = [
        {
          id: 1,
          engineer_id: 1,
          project_id: "PRJ001",
          start_date: "2026-10-04", // Today
          end_date: "2026-12-31",
          settlement_type: "RANGE",
          base_rate: 600000,
          effort: "160",
          is_active: true,
          project_name: "Project A",
          client_name: "Client A",
          engineer_name: "Engineer 1",
        },
      ];
      const count = calculateAssignmentCount(1, boundaryContracts);
      expect(count).toBe(1);
    });

    it("should handle boundary dates correctly - end date inclusive", () => {
      const boundaryContracts: ClientContractRow[] = [
        {
          id: 1,
          engineer_id: 1,
          project_id: "PRJ001",
          start_date: "2026-01-01",
          end_date: "2026-10-04", // Today
          settlement_type: "RANGE",
          base_rate: 600000,
          effort: "160",
          is_active: true,
          project_name: "Project A",
          client_name: "Client A",
          engineer_name: "Engineer 1",
        },
      ];
      const count = calculateAssignmentCount(1, boundaryContracts);
      expect(count).toBe(1);
    });
  });

  describe("getProposalPartners", () => {
    const mockPartnerContracts: ContractRow[] = [
      {
        id: 1,
        engineer_name: "Engineer 1",
        partner_name: "Partner A",
        project_name: "Project 1",
        settlement_type: "RANGE",
        base_rate: 600000,
        effort: "160",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        is_active: true,
      },
      {
        id: 2,
        engineer_name: "Engineer 1",
        partner_name: "Partner B",
        project_name: "Project 2",
        settlement_type: "RANGE",
        base_rate: 700000,
        effort: "160",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        is_active: true,
      },
      {
        id: 3,
        engineer_name: "Engineer 1",
        partner_name: "Partner A", // Duplicate partner
        project_name: "Project 3",
        settlement_type: "RANGE",
        base_rate: 650000,
        effort: "160",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        is_active: true,
      },
      {
        id: 4,
        engineer_name: "Engineer 2",
        partner_name: "Partner C",
        project_name: "Project 4",
        settlement_type: "RANGE",
        base_rate: 500000,
        effort: "160",
        start_date: "2026-01-01",
        end_date: "2026-12-31",
        is_active: true,
      },
    ];

    // 「今日」を 2026-10-04 に固定する。対象のコードは new Date() で今日を取るため、Date.now の差し替えでは効かず、
    // 実行日によって結果が変わっていた（2026-10-05 以降は失敗。DEMO-000152）。
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date("2026-10-04T03:00:00Z"));
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it("should return comma-separated partner names for active contracts", () => {
      const partners = getProposalPartners("Engineer 1", mockPartnerContracts);
      const partnerList = partners.split(", ");
      expect(partnerList).toContain("Partner A");
      expect(partnerList).toContain("Partner B");
      expect(partnerList).toHaveLength(2); // Duplicates should be removed
    });

    it("should return empty string for engineer with no active contracts", () => {
      const partners = getProposalPartners("Engineer 99", mockPartnerContracts);
      expect(partners).toBe("");
    });

    it("should not include partners from other engineers", () => {
      const partners = getProposalPartners("Engineer 1", mockPartnerContracts);
      expect(partners).not.toContain("Partner C");
    });

    it("should remove duplicate partner names", () => {
      const partners = getProposalPartners("Engineer 1", mockPartnerContracts);
      const partnerList = partners.split(", ");
      const partnerACount = partnerList.filter((p) => p === "Partner A").length;
      expect(partnerACount).toBe(1);
    });
  });
});
