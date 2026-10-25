import { describe, it, expect } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { refreshAfterOwnerChange } from "@/lib/owner-transfer";

describe("refreshAfterOwnerChange", () => {
  it("担当を変えたら、今の担当だけでなく取引先・案件の一覧も読み直す", async () => {
    const qc = new QueryClient();
    const keys = [["owner-transfer", "current"], ["masters-data", "clients"], ["masters-data", "partners"], ["projects"]];
    for (const k of keys) qc.setQueryData(k, { v: 1 });
    await refreshAfterOwnerChange(qc);
    for (const k of keys) {
      expect(qc.getQueryState(k)?.isInvalidated, JSON.stringify(k)).toBe(true);
    }
  });
});
