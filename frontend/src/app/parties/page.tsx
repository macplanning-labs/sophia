"use client";

import { Suspense } from "react";
import { PartiesPanel } from "@/components/parties/parties-panel";

/** 取引先(クライアント・パートナー)。サイドバー: 設定 > 取引先(マスタメンテの右隣) */
export default function PartiesPage() {
  return (
    <Suspense fallback={<div className="p-6 text-sm text-muted-foreground">読み込み中...</div>}>
      <div className="p-6">
        <PartiesPanel />
      </div>
    </Suspense>
  );
}
