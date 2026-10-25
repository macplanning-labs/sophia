import { describe, it, expect } from "vitest";
import { filterSentEmails, formatSentAt } from "@/lib/sent-emails";

describe("formatSentAt", () => {
  it("UTC を日本時間にする", () => {
    expect(formatSentAt("2026-10-08T11:55:55.084138Z")).toBe("2026-10-08 20:55");
    expect(formatSentAt("2026-10-08T20:00:00Z")).toBe("2026-10-09 05:00");
  });
  it("読めない値はそのまま", () => {
    expect(formatSentAt("not a date")).toBe("not a date");
  });
});

describe("filterSentEmails", () => {
  const rows = [
    { recipient: "a@example.co.jp", subject: "注文書送信の件" },
    { recipient: "b@partner.jp", subject: "稼働報告リマインド" },
  ];
  it("宛先・件名で絞り込む。空なら全件", () => {
    expect(filterSentEmails(rows, "注文書")).toHaveLength(1);
    expect(filterSentEmails(rows, "PARTNER")).toHaveLength(1);
    expect(filterSentEmails(rows, " ")).toHaveLength(2);
  });
});
