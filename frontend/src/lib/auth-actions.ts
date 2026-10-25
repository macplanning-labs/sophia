/**
 * auth-actions.ts - Authentication and theme-related actions
 *
 * Shared functions for logout and theme toggle
 */

import { markJustLoggedOut } from "@/lib/sso-login";

/**
 * ログアウト処理
 */
export async function handleLogout(): Promise<void> {
  // ログイン画面で、社内アカウントを自動で試さないようにする(すぐ入り直してしまうため)
  markJustLoggedOut();
  try {
    await fetch("/api/v1/auth/logout", {
      method: "POST",
      credentials: "include",
    });
  } catch {
    // エラーが起きても /login にリダイレクトする
  }
  // eslint-disable-next-line @next/next/no-location-assign-relative-destination
  window.location.href = "/login";
}
