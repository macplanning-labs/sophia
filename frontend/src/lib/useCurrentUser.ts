// lib/useCurrentUser.ts — ログインユーザーのロール情報取得フック
//
// サイドバーのメニュー出し分けや、画面内のAdmin専用アクションの表示制御に使う。
// GET /api/v1/auth/me（employee_routes配下 = Admin/Employeeのみ到達）をラップする。
//
// 使い方:
//   const { isAdmin, isLoading, user } = useCurrentUser();
//   if (isAdmin) { ... }

"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchCurrentUser, type CurrentUser } from "./api";

export function useCurrentUser() {
  const { data, isLoading, isError } = useQuery<CurrentUser>({
    queryKey: ["current-user"],
    queryFn: fetchCurrentUser,
    // staleTime: 0 が重要。ログインはSPA遷移(router.push)であり、ログアウトのようなフルリロードを
    // 挟まないため、staleTimeを長く取るとブラウザタブを閉じずに別アカウントへログインし直した際に
    // 古いロール（前のアカウントのADMIN/EMPLOYEE）がキャッシュから返り続け、サイドバー等の表示が
    // 実際のセッションと食い違うバグが発生する（要修正・実機確認済み）。login/page.tsxでの
    // queryClient.clear()と合わせて二重に対策している。
    staleTime: 0,
    retry: false,
  });

  return {
    user: data,
    isAdmin: data?.role === "ADMIN",
    /** 給与全件閲覧・控除再計算など（APIの can_view_all_payroll と揃える） */
    canViewAllPayroll: !!data?.can_view_all_payroll,
    isLoading,
    isError,
  };
}
