import { describe, it, expect } from "vitest";
import { avatarLabel } from "@/lib/avatar";

describe("avatarLabel", () => {
  it("氏名の頭文字を返す", () => {
    expect(avatarLabel("山田花子")).toBe("山");
    expect(avatarLabel(" 鈴木一郎")).toBe("鈴");
    expect(avatarLabel("yamada")).toBe("Y");
    expect(avatarLabel("𠮷野")).toBe("𠮷");
  });

  it("氏名が無ければ null(人型のアイコンにする)", () => {
    expect(avatarLabel(null)).toBeNull();
    expect(avatarLabel(undefined)).toBeNull();
    expect(avatarLabel("")).toBeNull();
    expect(avatarLabel("   ")).toBeNull();
  });
});
