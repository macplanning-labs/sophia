import { describe, it, expect } from "vitest";
import { formatYen, formatPct, flowLabel, targetBadge, warningTone } from "@/lib/assignment-display";

describe("assignment-display", () => {
  it("金額と率の整形", () => {
    expect(formatYen(1234567)).toBe("¥1,234,567");
    expect(formatYen(-20000)).toBe("-¥20,000");
    expect(formatYen(null)).toBe("—");
    expect(formatPct(8.6)).toBe("8.6%");
    expect(formatPct(null)).toBe("—");
  });

  it("商流の表示", () => {
    expect(flowLabel("DIRECT")).toBe("直受け");
    expect(flowLabel("SUBCONTRACT")).toBe("下請け");
    expect(flowLabel(null)).toBe("商流未設定");
  });

  it("目安との比較の表示", () => {
    expect(targetBadge("OK", 0.6, 8)).toEqual({ tone: "ok", text: "目安8% 以上" });
    expect(targetBadge("BELOW", -2.3, 8)).toEqual({ tone: "warn", text: "目安8% を2.3pt 下回る" });
    expect(targetBadge("UNSET", null, null).tone).toBe("muted");
    expect(targetBadge("NOT_APPLICABLE", null, null).text).toBe("—");
  });

  it("赤字だけが強い警告", () => {
    expect(warningTone("LOSS")).toBe("danger");
    expect(warningTone("RANGE_MISMATCH")).toBe("caution");
  });
});
