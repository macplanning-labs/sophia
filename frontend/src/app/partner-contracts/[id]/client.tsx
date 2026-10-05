"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useDynamicId } from "@/lib/utils";

/** 発注契約詳細は廃止。一覧の編集モーダルへリダイレクトする。 */
export default function PartnerContractDetailRedirect() {
  const id = useDynamicId();
  const router = useRouter();

  useEffect(() => {
    if (!id) return;
    router.replace(`/partner-contracts?edit=${encodeURIComponent(id)}`);
  }, [id, router]);

  return (
    <div className="p-8 text-sm text-muted-foreground">
      発注契約一覧の編集画面へ移動しています...
    </div>
  );
}
