"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useDynamicId } from "@/lib/utils";

/** 受注契約詳細は廃止。一覧の編集モーダルへリダイレクトする。 */
export default function ClientContractDetailRedirect() {
  const id = useDynamicId();
  const router = useRouter();

  useEffect(() => {
    if (!id) return;
    router.replace(`/client-contracts?edit=${encodeURIComponent(id)}`);
  }, [id, router]);

  return (
    <div className="p-8 text-sm text-muted-foreground">
      受注契約一覧の編集画面へ移動しています...
    </div>
  );
}
