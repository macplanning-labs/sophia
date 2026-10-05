import { afterEach, describe, expect, it } from "vitest";
import { render, screen, cleanup, act, fireEvent } from "@testing-library/react";
import { SessionExpiredModal } from "@/components/session-expired-modal";
import { notifySessionExpired } from "@/lib/session-expired";

describe("SessionExpiredModal", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders nothing until a session-expired notification arrives", () => {
    render(<SessionExpiredModal />);
    expect(screen.queryByText("セッションの期限が切れました")).toBeNull();
  });

  it("shows the modal with a single non-dismissible CTA after notification", () => {
    render(<SessionExpiredModal />);

    act(() => {
      notifySessionExpired("/login");
    });

    expect(screen.getByText("セッションの期限が切れました")).toBeDefined();
    expect(screen.getByRole("button", { name: "ログイン画面へ" })).toBeDefined();
    // 閉じるボタンは提供しない（背景クリック等で解除できてはいけない）
    expect(screen.queryByRole("button", { name: /閉じる|キャンセル/ })).toBeNull();
  });

  it("navigates to the given login_url with the current path as the redirect param", async () => {
    const originalLocation = window.location;
    // jsdomのlocationはreadonlyなので defineProperty で差し替える
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...originalLocation, pathname: "/settlement", search: "?month=2026-08", href: "" },
    });

    render(<SessionExpiredModal />);
    act(() => {
      notifySessionExpired("/portal/login");
    });

    fireEvent.click(screen.getByRole("button", { name: "ログイン画面へ" }));

    expect(window.location.href).toBe(
      `/portal/login?redirect=${encodeURIComponent("/settlement?month=2026-08")}`
    );

    Object.defineProperty(window, "location", { configurable: true, value: originalLocation });
  });
});
