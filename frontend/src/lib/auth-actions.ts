/**
 * auth-actions.ts - Authentication and theme-related actions
 *
 * Shared functions for logout and theme toggle
 */

/**
 * ログアウト処理
 */
export async function handleLogout(): Promise<void> {
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

/**
 * テーマ切り替え
 * @param currentTheme 現在のテーマ ("dark" | "light")
 * @returns 新しいテーマ
 */
export function toggleTheme(currentTheme: "dark" | "light"): "dark" | "light" {
  return currentTheme === "dark" ? "light" : "dark";
}
