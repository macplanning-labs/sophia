import { describe, it, expect } from "vitest";
import { monthFromParam } from "@/lib/settlement-month";

describe("monthFromParam", () => {
  it("YYYY-MM と YYYY-MM-DD を YYYY-MM にする", () => {
    expect(monthFromParam("2026-09")).toBe("2026-09");
    expect(monthFromParam("2026-09-01")).toBe("2026-09");
  });
  it("読めない値は null", () => {
    expect(monthFromParam(null)).toBeNull();
    expect(monthFromParam("")).toBeNull();
    expect(monthFromParam("2026-13")).toBeNull();
    expect(monthFromParam("9月")).toBeNull();
  });
});
