import { describe, it, expect } from "vitest";
import {
  defaultPeriod, initialForm, validateForm, toPreviewRequest, toCreateRequest, previewKey, type AssignmentForm,
} from "@/lib/assignment-form";

const partnerEng = { id: 7, name: "山田", affiliation_type: "PARTNER", partner_id: "0000000003" };
const employeeEng = { id: 8, name: "鈴木", affiliation_type: "EMPLOYEE", partner_id: null };

function filled(f: AssignmentForm): AssignmentForm {
  return {
    ...f,
    client: { ...f.client, base_rate: "700000" },
    partner: f.partner ? { ...f.partner, base_rate: "640000" } : null,
  };
}

describe("defaultPeriod", () => {
  it("来月1日から6か月後の月末まで", () => {
    expect(defaultPeriod(new Date(2026, 9, 4))).toEqual({ start: "2026-11-01", end: "2027-04-30" });
    expect(defaultPeriod(new Date(2026, 11, 20))).toEqual({ start: "2027-01-01", end: "2027-06-30" });
  });
});

describe("initialForm", () => {
  it("パートナー要員は発注側を持ち、所属パートナーを提案元の初期値にする", () => {
    const f = initialForm(partnerEng, "PRJ1", new Date(2026, 9, 4));
    expect(f.staff_type).toBe("PARTNER");
    expect(f.partner?.partner_id).toBe("0000000003");
    expect(f.client.lower_limit_hours).toBe("140");
  });

  it("自社社員は発注側を持たない", () => {
    const f = initialForm(employeeEng, "PRJ1", new Date(2026, 9, 4));
    expect(f.staff_type).toBe("EMPLOYEE");
    expect(f.partner).toBeNull();
  });
});

describe("validateForm", () => {
  it("単価が空なら受注の単価エラー", () => {
    expect(validateForm(initialForm(employeeEng, "P", new Date(2026, 9, 4)))).toContain("受注の単価");
  });
  it("揃っていれば OK", () => {
    expect(validateForm(filled(initialForm(employeeEng, "P", new Date(2026, 9, 4))))).toBeNull();
    expect(validateForm(filled(initialForm(partnerEng, "P", new Date(2026, 9, 4))))).toBeNull();
  });
  it("パートナー要員は提案元と発注の単価が必要", () => {
    const f = filled(initialForm(partnerEng, "P", new Date(2026, 9, 4)));
    expect(validateForm({ ...f, partner: { ...f.partner!, partner_id: "" } })).toContain("提案元パートナー");
    expect(validateForm({ ...f, partner: { ...f.partner!, base_rate: "" } })).toContain("発注の単価");
  });
  it("期間と精算幅", () => {
    const f = filled(initialForm(employeeEng, "P", new Date(2026, 9, 4)));
    expect(validateForm({ ...f, end_date: "2026-01-01" })).toContain("終了日");
    expect(validateForm({ ...f, client: { ...f.client, lower_limit_hours: "200" } })).toContain("精算幅");
  });
});

describe("API リクエスト", () => {
  it("入力が揃うまで試算を呼ばない", () => {
    expect(toPreviewRequest(initialForm(partnerEng, "P", new Date(2026, 9, 4)))).toBeNull();
    const half = initialForm(partnerEng, "P", new Date(2026, 9, 4));
    half.client.base_rate = "700000";
    expect(toPreviewRequest(half)).toBeNull(); // 発注の単価がまだ
    expect(toPreviewRequest(filled(initialForm(partnerEng, "P", new Date(2026, 9, 4))))).not.toBeNull();
  });

  it("試算リクエストは数値で、自社社員は発注を含まない", () => {
    const r = toPreviewRequest(filled(initialForm(employeeEng, "PRJ1", new Date(2026, 9, 4))))!;
    expect(r.client_contract).toMatchObject({ base_rate: 700000, effort: 1, lower_limit_hours: 140, upper_limit_hours: 180 });
    expect(r.partner_contract).toBeUndefined();
    expect(r.engineer_id).toBe(8);
  });

  it("作成リクエストは区分・期間・提案元パートナーを含む", () => {
    const r = toCreateRequest(filled(initialForm(partnerEng, "PRJ1", new Date(2026, 9, 4))));
    expect(r).toMatchObject({ project_id: "PRJ1", staff_type: "PARTNER", engineer_id: 7 });
    expect(r.client_contract).toMatchObject({ start_date: "2026-11-01", end_date: "2027-04-30", base_rate: 700000 });
    expect(r.partner_contract).toMatchObject({ partner_id: "0000000003", base_rate: 640000 });
  });

  it("同じ入力は同じ指紋、違えば別", () => {
    const a = filled(initialForm(employeeEng, "P", new Date(2026, 9, 4)));
    const b = { ...a, client: { ...a.client, base_rate: "710000" } };
    expect(previewKey(a)).toBe(previewKey({ ...a }));
    expect(previewKey(a)).not.toBe(previewKey(b));
    expect(previewKey(initialForm(employeeEng, "P", new Date(2026, 9, 4)))).toBeNull();
  });
});
