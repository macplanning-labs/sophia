import { describe, it, expect } from "vitest";
import { clientOwnerProblem, CLIENT_WITHOUT_OWNER_MESSAGE } from "@/lib/project-wizard";

describe("clientOwnerProblem", () => {
  const options = [
    { value: 1, label: "担当なし", has_owner: false },
    { value: 2, label: "担当あり", has_owner: true },
  ];

  it("担当の無い顧客を選ぶと案内を返す", () => {
    expect(clientOwnerProblem(options, "1")).toBe(CLIENT_WITHOUT_OWNER_MESSAGE);
  });

  it("担当のある顧客・未選択・一覧に無い値は問題なし", () => {
    expect(clientOwnerProblem(options, "2")).toBeUndefined();
    expect(clientOwnerProblem(options, "")).toBeUndefined();
    expect(clientOwnerProblem(options, "9")).toBeUndefined();
  });
});
