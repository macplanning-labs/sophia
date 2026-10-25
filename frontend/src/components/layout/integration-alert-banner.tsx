"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle } from "lucide-react";
import { fetchIntegrationAlert } from "@/lib/api";
import { isChromeHidden } from "@/lib/layout-paths";
import { summarizeAlert } from "@/lib/integration-alert";
import { useCurrentUser } from "@/lib/useCurrentUser";

/** 外部連携に問題があるとき、管理者だけに出す赤い帯(ヘッダーの下。閉じられない。直すと自動で消える)。DEMO-000216。 */
export function IntegrationAlertBanner() {
  const pathname = usePathname();
  const { isAdmin } = useCurrentUser();
  const { data } = useQuery({
    queryKey: ["integration-alert"],
    queryFn: fetchIntegrationAlert,
    enabled: isAdmin,
    refetchInterval: 5 * 60 * 1000,
    retry: false,
  });
  if (!isAdmin || isChromeHidden(pathname) || !data) return null;
  const summary = summarizeAlert(data.problems, data.checked_at);
  if (!summary) return null;

  return (
    <div
      role="alert"
      className="flex shrink-0 items-center gap-3 border-b border-red-800 bg-red-950/70 px-6 py-2 pl-14 text-sm text-red-200 lg:pl-6"
    >
      <AlertTriangle className="h-4 w-4 shrink-0 text-red-400" aria-hidden="true" />
      <div className="min-w-0">
        <p className="truncate font-medium">{summary.title}</p>
        <p className="truncate text-xs text-red-300">{summary.detail}</p>
      </div>
      <Link
        href="/settings/integrations"
        className="ml-auto shrink-0 whitespace-nowrap rounded-md bg-red-700 px-3 py-1 text-xs text-white hover:bg-red-600"
      >
        外部連携を開く →
      </Link>
    </div>
  );
}
