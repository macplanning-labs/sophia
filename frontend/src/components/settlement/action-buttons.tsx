"use client";

import { Button } from "@/components/ui/button";
import { FileUp, FileDown, Loader2 } from "lucide-react";

interface Props {
  selectedCount: number;
  totalCount: number;
  onIssueInvoices: () => void;
  onIssueNotices: () => void;
  isIssuingInvoices: boolean;
  isIssuingNotices: boolean;
}

export function ActionButtons({
  selectedCount,
  totalCount,
  onIssueInvoices,
  onIssueNotices,
  isIssuingInvoices,
  isIssuingNotices,
}: Props) {
  return (
    <div className="flex items-center gap-4">
      <Button
        onClick={onIssueInvoices}
        disabled={selectedCount === 0 || isIssuingInvoices}
        className="bg-emerald-600 hover:bg-emerald-700 text-foreground"
      >
        {isIssuingInvoices ? (
          <Loader2 className="w-4 h-4 mr-2 animate-spin" />
        ) : (
          <FileUp className="w-4 h-4 mr-2" />
        )}
        選択した請求書を一括発行
      </Button>
      <Button
        onClick={onIssueNotices}
        disabled={selectedCount === 0 || isIssuingNotices}
        className="bg-orange-600 hover:bg-orange-700 text-foreground"
      >
        {isIssuingNotices ? (
          <Loader2 className="w-4 h-4 mr-2 animate-spin" />
        ) : (
          <FileDown className="w-4 h-4 mr-2" />
        )}
        支払通知書を一括発行（会社単位集約）
      </Button>
    </div>
  );
}
