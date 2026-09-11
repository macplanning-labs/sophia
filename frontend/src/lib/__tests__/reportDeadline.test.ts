import { describe, expect, it } from "vitest";
import {
  calculateDeadline,
  ReportDeadlineCalculationError,
  reportDeadlineSchema,
} from "@/lib/reportDeadline";
import type { ProjectReportDeadline } from "@/lib/types";

// 2026/7/20は「海の日」（第3月曜）。この関数で祝日として扱う（土日祝の繰り越しテスト用）。
function isJapaneseHoliday(date: Date): boolean {
  return date.getFullYear() === 2026 && date.getMonth() === 6 && date.getDate() === 20;
}

// calculateDeadlineはローカルタイムのDateを返すため、toISOString()（UTC変換）で比較すると
// 実行環境のタイムゾーンによって日付がずれる。ローカルのY/M/Dをそのまま文字列化して比較する。
function toLocalDateString(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

describe("reportDeadlineSchema", () => {
  it("accepts a RELATIVE value with no holiday rule", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "RELATIVE",
      report_deadline_value: 3,
      report_deadline_holiday_rule: null,
    });
    expect(result.success).toBe(true);
  });

  it("rejects a negative RELATIVE value", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "RELATIVE",
      report_deadline_value: -1,
      report_deadline_holiday_rule: null,
    });
    expect(result.success).toBe(false);
  });

  it("requires holiday_rule when type is FIXED_DAY", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 20,
      report_deadline_holiday_rule: null,
    });
    expect(result.success).toBe(false);
  });

  it("accepts FIXED_DAY with a holiday rule set", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 20,
      report_deadline_holiday_rule: "NEXT_BUSINESS_DAY",
    });
    expect(result.success).toBe(true);
  });

  it("rejects FIXED_DAY values outside 1-31", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 32,
      report_deadline_holiday_rule: "NEXT_BUSINESS_DAY",
    });
    expect(result.success).toBe(false);
  });

  it("allows a null value for both types (project setting unset, falls back to contract)", () => {
    const result = reportDeadlineSchema.safeParse({
      report_deadline_type: "RELATIVE",
      report_deadline_value: null,
      report_deadline_holiday_rule: null,
    });
    expect(result.success).toBe(true);
  });
});

describe("calculateDeadline", () => {
  it("counts back N business days from month end for RELATIVE", () => {
    // 2026年7月の月末は7/31(金)。3営業日前 → 7/30(木)→7/29(水)→7/28(火)
    const settings: ProjectReportDeadline = {
      report_deadline_type: "RELATIVE",
      report_deadline_value: 3,
      report_deadline_holiday_rule: null,
    };
    const result = calculateDeadline(2026, 7, settings, isJapaneseHoliday);
    expect(toLocalDateString(result)).toBe("2026-07-28");
  });

  it("returns the fixed day directly when it's a business day", () => {
    const settings: ProjectReportDeadline = {
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 8, // 2026/7/8 水曜・非祝日
      report_deadline_holiday_rule: "NEXT_BUSINESS_DAY",
    };
    const result = calculateDeadline(2026, 7, settings, isJapaneseHoliday);
    expect(toLocalDateString(result)).toBe("2026-07-08");
  });

  it("slides back over a holiday when rule is PREVIOUS_BUSINESS_DAY", () => {
    // 2026/7/20は海の日（月曜祝日）。前営業日ルールなら7/19(日)→7/18(土)→7/17(金)
    const settings: ProjectReportDeadline = {
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 20,
      report_deadline_holiday_rule: "PREVIOUS_BUSINESS_DAY",
    };
    const result = calculateDeadline(2026, 7, settings, isJapaneseHoliday);
    expect(toLocalDateString(result)).toBe("2026-07-17");
  });

  it("slides forward over a holiday when rule is NEXT_BUSINESS_DAY", () => {
    const settings: ProjectReportDeadline = {
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 20,
      report_deadline_holiday_rule: "NEXT_BUSINESS_DAY",
    };
    const result = calculateDeadline(2026, 7, settings, isJapaneseHoliday);
    expect(toLocalDateString(result)).toBe("2026-07-21");
  });

  it("throws when FIXED_DAY value exceeds days in month", () => {
    const settings: ProjectReportDeadline = {
      report_deadline_type: "FIXED_DAY",
      report_deadline_value: 31,
      report_deadline_holiday_rule: "NEXT_BUSINESS_DAY",
    };
    // 2026年4月は30日まで
    expect(() => calculateDeadline(2026, 4, settings, isJapaneseHoliday)).toThrow(
      ReportDeadlineCalculationError,
    );
  });

  it("throws when value is null (caller should fall back to contract-level settings)", () => {
    const settings: ProjectReportDeadline = {
      report_deadline_type: "RELATIVE",
      report_deadline_value: null,
      report_deadline_holiday_rule: null,
    };
    expect(() => calculateDeadline(2026, 7, settings, isJapaneseHoliday)).toThrow(
      ReportDeadlineCalculationError,
    );
  });
});
