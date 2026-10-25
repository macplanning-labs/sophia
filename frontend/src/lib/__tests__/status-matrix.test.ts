import { describe, expect, it } from "vitest";
import { NEXT_ACTION } from "@/lib/status-matrix";

describe("NEXT_ACTION(月別状況の次の操作の移動先)", () => {
  it("「支払通知を作成」は、支払通知書を発行できる月次確定の支払タブへ移る(DEMO-000189)", () => {
    expect(NEXT_ACTION.CREATE_NOTICE.href).toBe("/settlement?view=payment");
  });

  it("どの操作も、一覧だけで作成できない /notices へは移らない", () => {
    for (const [key, action] of Object.entries(NEXT_ACTION)) {
      expect(action.href.startsWith("/notices"), key).toBe(false);
    }
  });
});
