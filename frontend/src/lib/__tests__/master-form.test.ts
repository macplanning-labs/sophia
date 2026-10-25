/**
 * master-form.test.ts — マスタ定義からフォームを作る共通関数(取引先の編集フォームとマスタメンテで共通)
 */

import { describe, it, expect } from "vitest";
import {
  groupPartyColumns,
  missingRequired,
  optionLabel,
  toMasterFormData,
  toMasterPayload,
  type MasterFormColumnMeta,
} from "@/lib/master-form";

const col = (name: string, input_type = "text", extra: Partial<MasterFormColumnMeta> = {}): MasterFormColumnMeta => ({
  name, label: name, input_type, required: false, is_pk: false, ...extra,
});

describe("groupPartyColumns", () => {
  it("クライアントの項目を区切りに分け、決まっていない項目は「その他」に入れる(取りこぼさない)", () => {
    const cols = [col("name"), col("billing_unit", "select"), col("edi_system_type", "select"), col("registration_no"), col("new_field")];
    const sections = groupPartyColumns("client", cols);
    expect(sections.map((s) => [s.title, s.cols.map((c) => c.name)])).toEqual([
      ["基本情報", ["name"]],
      ["請求・送付先", ["billing_unit"]],
      ["EDI", ["edi_system_type"]],
      ["インボイス・Peppol", ["registration_no"]],
      ["その他", ["new_field"]],
    ]);
    expect(sections.flatMap((s) => s.cols)).toHaveLength(cols.length);
  });

  it("パートナーは振込口座の区切りを持つ。項目の無い区切りは出さない", () => {
    const sections = groupPartyColumns("partner", [col("name"), col("bank_name"), col("account_type", "select")]);
    expect(sections.map((s) => s.title)).toEqual(["基本情報", "振込口座"]);
  });
});

describe("toMasterFormData / toMasterPayload", () => {
  const cols = [col("name"), col("invoice_issued_by_client", "checkbox"), col("closing_day", "number")];

  it("null・未設定は空文字、真偽値は文字列にする", () => {
    expect(toMasterFormData(cols, { name: "A社", invoice_issued_by_client: true, closing_day: null })).toEqual({
      name: "A社", invoice_issued_by_client: "true", closing_day: "",
    });
    expect(toMasterFormData(cols)).toEqual({ name: "", invoice_issued_by_client: "", closing_day: "" });
  });

  it("保存では、チェックボックスは真偽値、数値は数にする", () => {
    expect(toMasterPayload(cols, { name: "A社", invoice_issued_by_client: "false", closing_day: "25" })).toEqual({
      name: "A社", invoice_issued_by_client: false, closing_day: 25,
    });
  });
});

describe("missingRequired / optionLabel", () => {
  it("空の必須項目の名前を返す", () => {
    const cols = [col("name", "text", { required: true, label: "名前" }), col("email")];
    expect(missingRequired(cols, { name: "  ", email: "" })).toEqual(["名前"]);
    expect(missingRequired(cols, { name: "A社", email: "" })).toEqual([]);
  });

  it("選択肢の値を表示名にする。空は —、知らない値はそのまま", () => {
    const c = col("billing_unit", "select", { options: [{ value: "PROJECT", label: "案件ごと" }] });
    expect(optionLabel(c, "PROJECT")).toBe("案件ごと");
    expect(optionLabel(c, "")).toBe("—");
    expect(optionLabel(c, "OTHER")).toBe("OTHER");
    expect(optionLabel(undefined, "EMAIL")).toBe("EMAIL");
  });
});
