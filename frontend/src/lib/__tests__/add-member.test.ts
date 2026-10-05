import { describe, it, expect } from "vitest";
import {
  validateAddMember, buildMemberPayload, buildPartnerPayload, buildAttachPayload, attachCandidates,
  type AddMemberForm,
} from "@/lib/add-member";
import type { MemberRow } from "@/lib/types";

const base: AddMemberForm = {
  classification: "PARTNER", mode: "new", name: "山田 太郎", name_kana: "", email: "",
  partner_id: "0000000001", newPartner: null, existing_id: "",
};

const member = (o: Partial<MemberRow>): MemberRow => ({
  id: 1, name: "要員", name_kana: "カナ", affiliation_type: "PARTNER", partner_id: null,
  employee_id: "", email: "a@example.com", is_active: true, ...o,
});

describe("validateAddMember", () => {
  it("新規の要員は氏名が必須", () => {
    expect(validateAddMember({ ...base, name: "  " })).toContain("氏名");
  });

  it("パートナー要員は、既存のパートナーを選ぶか、新しいパートナーの名称が必要", () => {
    expect(validateAddMember({ ...base, partner_id: "" })).toContain("パートナーを選択");
    expect(validateAddMember({ ...base, partner_id: "", newPartner: { name: "", contact_person: "", email: "" } })).toContain("名称");
    expect(validateAddMember({ ...base, partner_id: "", newPartner: { name: "新パートナー", contact_person: "", email: "" } })).toBeNull();
    expect(validateAddMember(base)).toBeNull();
  });

  it("登録済みの要員を選ぶ方法では、氏名は不要で、要員の選択が必要", () => {
    const ex = { ...base, mode: "existing" as const, name: "" };
    expect(validateAddMember(ex)).toContain("登録済みの要員");
    expect(validateAddMember({ ...ex, existing_id: "12" })).toBeNull();
    expect(validateAddMember({ ...ex, existing_id: "12", partner_id: "" })).toContain("パートナーを選択");
  });

  it("自社社員はパートナー不要(常に新規)", () => {
    expect(validateAddMember({ ...base, classification: "EMPLOYEE", partner_id: "" })).toBeNull();
    expect(validateAddMember({ ...base, classification: "EMPLOYEE", partner_id: "", name: "" })).toContain("氏名");
  });
});

describe("buildMemberPayload", () => {
  it("パートナー要員は所属パートナーを持つ", () => {
    expect(buildMemberPayload({ ...base, name: " 山田 太郎 " }, "0000000009"))
      .toMatchObject({ name: "山田 太郎", affiliation_type: "PARTNER", partner_id: "0000000009" });
  });

  it("自社社員はパートナーを持たない", () => {
    const p = buildMemberPayload({ ...base, classification: "EMPLOYEE" }, "0000000009");
    expect(p.affiliation_type).toBe("EMPLOYEE");
    expect(p.partner_id).toBeUndefined();
  });
});

describe("buildAttachPayload", () => {
  it("いまの値を引き継ぎ、所属パートナーだけを変える(送らない項目が空にならないように)", () => {
    const p = buildAttachPayload(member({ id: 5, name: "既存 花子", email: "h@example.com", is_active: true }), "0000000002");
    expect(p).toEqual({
      name: "既存 花子", name_kana: "カナ", affiliation_type: "PARTNER",
      partner_id: "0000000002", email: "h@example.com", is_active: true,
    });
  });
});

describe("attachCandidates", () => {
  const names = { "0000000001": "株式会社A", "0000000002": "株式会社B" };
  it("有効なパートナー要員のうち、選んだパートナーにまだ所属していない人だけ", () => {
    const list = [
      member({ id: 1, name: "未所属", partner_id: null }),
      member({ id: 2, name: "Bの人", partner_id: "0000000002" }),
      member({ id: 3, name: "Aの人", partner_id: "0000000001" }),
      member({ id: 4, name: "自社", affiliation_type: "EMPLOYEE" }),
      member({ id: 5, name: "無効", is_active: false }),
    ];
    const c = attachCandidates(list, "0000000001", names);
    expect(c.map((x) => x.value)).toEqual(["1", "2"]);
    expect(c[0].label).toBe("未所属(所属なし)");
    expect(c[1].label).toBe("Bの人(所属: 株式会社B)");
    expect(c[1].currentPartnerName).toBe("株式会社B");
  });
});

describe("buildPartnerPayload", () => {
  it("前後の空白を除く", () => {
    expect(buildPartnerPayload({ name: " 株式会社A ", contact_person: " 担当 ", email: " a@example.com " }))
      .toEqual({ name: "株式会社A", contact_person: "担当", email: "a@example.com" });
  });
});
