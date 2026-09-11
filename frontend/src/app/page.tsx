"use client";

import { useQuery } from "@tanstack/react-query";
import { fetchDashboard } from "@/lib/api";
import { Button } from "@/components/ui/button";
import Link from "next/link";
import { Clock } from "lucide-react";
import { EdiPanel, MailPanel } from "@/components/dashboard/operations-panel";
import { ProjectSection } from "@/components/dashboard/project-section";
import { MailThreadCards } from "@/components/dashboard/mail-thread-cards";
import { PageHeader } from "@/components/ui/page-header";

export default function DashboardPage() {
  const { data, isLoading } = useQuery({
    queryKey: ["dashboard"],
    queryFn: fetchDashboard,
    refetchInterval: 60 * 1000, // 1分ごと自動更新
  });

  return (
    <div className="p-6 space-y-6">
      <PageHeader
        actions={
          <div className="flex items-center gap-2 flex-wrap justify-end">
            <Link href="/settlement">
              <Button variant="outline" size="sm" className="border-border text-muted-foreground text-xs gap-1">
                月次確定
              </Button>
            </Link>
            <Link href="/timesheets">
              <Button variant="outline" size="sm" className="border-border text-muted-foreground text-xs gap-1">
                <Clock className="w-3 h-3" /> 稼働報告
              </Button>
            </Link>
          </div>
        }
      />

      {/* プロジェクト別(最上部・主役) */}
      <ProjectSection
        partnerProgress={data?.partner_progress ?? []}
        clientProgress={data?.client_progress ?? []}
        progressLoading={isLoading}
      />

      {/* 取引先別メール要約カード */}
      <MailThreadCards />

      {/* EDI操作 + メール */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <EdiPanel />
        <MailPanel mailLogs={data?.mail_logs ?? []} />
      </div>
    </div>
  );
}
