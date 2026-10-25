/**
 * account-number-input.test.ts — 口座番号の入力検査(画面側)。サーバの AccountNumber::parse_input と同じ規則。DEMO-000233 S2-G2-4
 */

import { describe, it, expect } from "vitest";
import {
  ACCOUNT_NUMBER_INPUT_PROPS,
  accountNumberColumnError,
  accountNumberFormError,
  type MasterFormColumnMeta,
} from "@/lib/master-form";

const col = (name: string): MasterFormColumnMeta => ({ name, label: name, input_type: "text", required: false, is_pk: false });

describe("accountNumberFormError", () => {
  it("数字・全角数字・ハイフン・空白は受け付ける(保存のときサーバが数字だけに直す)", () => {
    for (const ok of ["1234567", "0012345", "１２３４５６７", "123-4567", "１２３－４５６７", " 123 4567 ", "１２３　４５６７", "", "   "]) {
      expect(accountNumberFormError(ok), `受け付ける: ${JSON.stringify(ok)}`).toBeNull();
    }
  });

  it("IME のハイフン(ー・―)とゼロ幅の空白は、ハイフンとして受け付ける(レビュー指摘 3)", () => {
    for (const ok of ["123ー4567", "123―4567", "123​4567", "１２３ー４５６７"]) {
      expect(accountNumberFormError(ok), `受け付ける: ${JSON.stringify(ok)}`).toBeNull();
    }
    // 数字が 1 つも無いハイフン類だけは、従来どおり拒否する
    for (const bad of ["ー", "―", "​"]) {
      expect(accountNumberFormError(bad), `拒否: ${JSON.stringify(bad)}`).toBe("口座番号は数字で入力してください");
    }
  });

  it("タブ・改行は拒否する(サーバの入力の集合と同じ)", () => {
    expect(accountNumberFormError("123\t4567")).toBe("口座番号は数字で入力してください");
    expect(accountNumberFormError("123\n4567")).toBe("口座番号は数字で入力してください");
  });

  it("20 桁まで受け付け、21 桁は拒否する", () => {
    expect(accountNumberFormError("1".repeat(20))).toBeNull();
    expect(accountNumberFormError("1".repeat(21))).toBe("口座番号は数字 1〜20 桁で入力してください");
  });

  it("英字・記号・制御文字・絵文字は拒否する(値は文言に含めない)", () => {
    for (const bad of ["12a4567", "ABC", "12.34", "12/34", "１２３ａ", "1\u0007" + "23", "1\t2", "12😀", "-", " - "]) {
      const msg = accountNumberFormError(bad);
      expect(msg, `拒否: ${JSON.stringify(bad)}`).toBe("口座番号は数字で入力してください");
    }
  });

  it("enc1: で始まる入力は拒否する", () => {
    expect(accountNumberFormError("enc1:k1:AAAA")).toBe("口座番号の形式が正しくありません");
    expect(accountNumberFormError("  enc1:")).toBe("口座番号の形式が正しくありません");
  });

  it("極端に長い不正入力も拒否する", () => {
    expect(accountNumberFormError("9".repeat(100000) + "x")).not.toBeNull();
  });
});

describe("accountNumberColumnError", () => {
  it("口座番号の欄が無い表では検査しない", () => {
    expect(accountNumberColumnError([col("name")], { name: "abc" })).toBeNull();
  });

  it("口座番号の欄だけを検査する", () => {
    const cols = [col("name"), col("account_number")];
    expect(accountNumberColumnError(cols, { name: "何でも", account_number: "1234567" })).toBeNull();
    expect(accountNumberColumnError(cols, { name: "何でも", account_number: "abc" })).not.toBeNull();
  });
});

describe("ACCOUNT_NUMBER_INPUT_PROPS", () => {
  it("数字の入力と桁数の上限を持つ", () => {
    expect(ACCOUNT_NUMBER_INPUT_PROPS.inputMode).toBe("numeric");
    expect(ACCOUNT_NUMBER_INPUT_PROPS.maxLength).toBeGreaterThanOrEqual(20);
    expect(ACCOUNT_NUMBER_INPUT_PROPS.pattern).toContain("0-9");
  });

  it("pattern は、サーバと同じ文字集合だけを受け付ける(タブは含めない)", () => {
    // HTML の pattern 属性は ^...$ で照合される。u フラグで、受け付ける文字と受け付けない文字を確かめる
    const re = new RegExp(`^${ACCOUNT_NUMBER_INPUT_PROPS.pattern}$`, "u");
    const accepted = ["0", "9", "０", "９", "-", "－", "‐", "–", "−", "ー", "―", "​", " ", "　", " ", "1-２ー3―4"];
    for (const ch of accepted) {
      expect(re.test(ch), `受け付ける: ${JSON.stringify(ch)}`).toBe(true);
    }
    for (const ch of ["\t", "\n", "a", "enc1:", "."]) {
      expect(re.test(ch), `受け付けない: ${JSON.stringify(ch)}`).toBe(false);
    }
  });
});
