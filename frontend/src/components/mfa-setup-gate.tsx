"use client";

import { useEffect } from "react";
import { usePathname, useRouter } from "next/navigation";
import { useCurrentUser } from "@/lib/useCurrentUser";

/** 社員で MFA 未登録ならセキュリティ設定へ誘導（パートナー導線は対象外） */
export function MfaSetupGate({ children }: { children: React.ReactNode }) {
  const { user, isLoading } = useCurrentUser();
  const router = useRouter();
  const pathname = usePathname();

  useEffect(() => {
    if (isLoading || !user?.mfa_setup_required) return;
    if (pathname?.startsWith("/settings/security")) return;
    if (
      pathname?.startsWith("/login")
      || pathname?.startsWith("/mfa")
      || pathname?.startsWith("/portal")
      || pathname?.startsWith("/invite")
      || pathname?.startsWith("/token")
    ) return;
    router.replace("/settings/security");
  }, [isLoading, user, pathname, router]);

  return <>{children}</>;
}
