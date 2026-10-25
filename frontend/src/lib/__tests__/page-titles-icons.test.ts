import { describe, it, expect } from "vitest";
import { resolvePageMeta } from "@/lib/page-titles";
import { SECTIONS } from "@/lib/section-tabs";

describe("一番上の帯のタイトル(DEMO-000197)", () => {
  // サイドバーのタブから行ける画面と、案件。請求書一覧は対象外(なくなる予定)
  const paths = [
    ...SECTIONS.flatMap((s) => s.tabs.map((t) => t.href)).filter((h) => h !== "/invoices"),
    "/projects",
  ];

  it("全画面にタイトルがあり、アイコンは線のアイコン(絵文字ではない)", () => {
    for (const p of paths) {
      const meta = resolvePageMeta(p);
      expect(meta, p).not.toBeNull();
      expect(meta?.lucideIcon, p).toBeTruthy();
      expect(meta as unknown as Record<string, unknown>, p).not.toHaveProperty("emoji");
    }
  });
});
