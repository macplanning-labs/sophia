import { describe, expect, it } from "vitest";
import { summarizeAlert } from "../integration-alert";

describe("summarizeAlert", () => {
  it("問題が無ければ帯は出さない", () => {
    expect(summarizeAlert([], "2026-10-12 08:45")).toBeNull();
    expect(summarizeAlert([], null)).toBeNull();
  });

  it("1件なら、その中身と最後の点検の時刻を出す", () => {
    expect(summarizeAlert(["メール送信: 鍵のファイルがありません"], "2026-10-12 08:45")).toEqual({
      title: "外部連携に問題があります(1件)",
      detail: "メール送信: 鍵のファイルがありません ・ 最後の点検 2026-10-12 08:45",
    });
  });

  it("複数なら、先頭の1件と「ほか○件」にまとめる", () => {
    const s = summarizeAlert(["a", "b", "c"], "2026-10-12 08:45");
    expect(s?.title).toBe("外部連携に問題があります(3件)");
    expect(s?.detail).toBe("a ほか 2件 ・ 最後の点検 2026-10-12 08:45");
  });

  it("点検の時刻が無くても出せる", () => {
    expect(summarizeAlert(["a"], null)?.detail).toBe("a");
  });
});
