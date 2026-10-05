import { describe, it, expect } from "vitest";
import { renderToString } from "react-dom/server";
import { renderHook } from "@testing-library/react";
import { useMounted } from "../useMounted";

describe("useMounted", () => {
  it("returns false during SSR", () => {
    // Simulate SSR by using renderToString
    const Component = () => {
      const mounted = useMounted();
      return <div>{mounted ? "mounted" : "not-mounted"}</div>;
    };

    const html = renderToString(<Component />);
    expect(html).toContain("not-mounted");
  });

  it("returns true after client hydration", () => {
    const { result } = renderHook(() => useMounted());
    expect(result.current).toBe(true);
  });
});
