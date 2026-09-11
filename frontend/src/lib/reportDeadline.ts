// lib/reportDeadline.ts — 案件（案件マスタ）の稼働報告提出期限
//
// バックエンド（src/domain/services/business_day.rs の calculate_deadline）と
// 同じルールをフロント側でも扱えるようにする。値の意味・エラー条件はRust側と揃えてある。

import { z } from "zod";
import type { ProjectReportDeadline, ReportDeadlineHolidayRule } from "./types";

// ── バリデーションスキーマ ──

export const reportDeadlineSchema = z
  .object({
    report_deadline_type: z.enum(["RELATIVE", "FIXED_DAY"]),
    report_deadline_value: z
      .number()
      .int()
      .nullable(),
    report_deadline_holiday_rule: z
      .enum(["PREVIOUS_BUSINESS_DAY", "NEXT_BUSINESS_DAY"])
      .nullable(),
  })
  .superRefine((data, ctx) => {
    if (data.report_deadline_value === null) {
      // プロジェクト単位の設定が未入力（契約側にフォールバック）。type/holiday_ruleは無視してよい。
      return;
    }

    if (data.report_deadline_type === "FIXED_DAY") {
      if (data.report_deadline_value < 1 || data.report_deadline_value > 31) {
        ctx.addIssue({
          code: "custom",
          path: ["report_deadline_value"],
          message: "FIXED_DAYの値は1〜31の範囲で指定してください",
        });
      }
      if (data.report_deadline_holiday_rule === null) {
        ctx.addIssue({
          code: "custom",
          path: ["report_deadline_holiday_rule"],
          message: "FIXED_DAYを指定する場合、休日調整ルールは必須です",
        });
      }
    } else {
      // RELATIVE
      if (data.report_deadline_value < 0) {
        ctx.addIssue({
          code: "custom",
          path: ["report_deadline_value"],
          message: "RELATIVEの値は0以上で指定してください",
        });
      }
    }
  });

export type ReportDeadlineFormValues = z.infer<typeof reportDeadlineSchema>;

// ── 期限日計算 ──

export class ReportDeadlineCalculationError extends Error {}

/**
 * 対象年月の稼働報告提出期限を計算する。
 *
 * `isHoliday` は祝日判定を外部から注入する（日本の祝日データを持つ既存実装があれば
 * それを渡す。テストではダミー関数で置き換え可能）。
 */
export function calculateDeadline(
  year: number,
  month: number, // 1-12
  settings: ProjectReportDeadline,
  isHoliday: (date: Date) => boolean,
): Date {
  if (month < 1 || month > 12) {
    throw new ReportDeadlineCalculationError(`無効な年月です: ${year}年${month}月`);
  }
  if (settings.report_deadline_value === null) {
    throw new ReportDeadlineCalculationError(
      "report_deadline_valueが未設定です（契約側の設定にフォールバックしてください）",
    );
  }

  const monthEnd = lastDayOfMonth(year, month);
  const value = settings.report_deadline_value;

  if (settings.report_deadline_type === "RELATIVE") {
    if (value < 0) {
      throw new ReportDeadlineCalculationError(`RELATIVEの値は0以上である必要があります: ${value}`);
    }
    return subtractBusinessDays(monthEnd, value, isHoliday);
  }

  // FIXED_DAY
  const holidayRule = settings.report_deadline_holiday_rule;
  if (holidayRule === null) {
    throw new ReportDeadlineCalculationError("FIXED_DAYを指定する場合、休日調整ルールは必須です");
  }

  const daysInMonth = monthEnd.getDate();
  if (value < 1 || value > daysInMonth) {
    throw new ReportDeadlineCalculationError(
      `FIXED_DAYの値が不正です（1〜${daysInMonth}の範囲で指定してください）: ${value}`,
    );
  }

  const base = new Date(year, month - 1, value);
  if (!isWeekendOrHoliday(base, isHoliday)) {
    return base;
  }

  return slideToBusinessDay(base, holidayRule, isHoliday);
}

function isWeekendOrHoliday(date: Date, isHoliday: (date: Date) => boolean): boolean {
  const day = date.getDay();
  return day === 0 || day === 6 || isHoliday(date);
}

function addDays(date: Date, days: number): Date {
  const d = new Date(date);
  d.setDate(d.getDate() + days);
  return d;
}

function lastDayOfMonth(year: number, month: number): Date {
  // 翌月1日の前日 = 当月末日
  return new Date(year, month, 0);
}

function subtractBusinessDays(
  date: Date,
  n: number,
  isHoliday: (date: Date) => boolean,
): Date {
  let d = date;
  let remaining = n;
  while (remaining > 0) {
    d = addDays(d, -1);
    if (!isWeekendOrHoliday(d, isHoliday)) {
      remaining -= 1;
    }
  }
  return d;
}

function slideToBusinessDay(
  date: Date,
  rule: ReportDeadlineHolidayRule,
  isHoliday: (date: Date) => boolean,
): Date {
  const step = rule === "PREVIOUS_BUSINESS_DAY" ? -1 : 1;
  let d = date;
  while (isWeekendOrHoliday(d, isHoliday)) {
    d = addDays(d, step);
  }
  return d;
}
