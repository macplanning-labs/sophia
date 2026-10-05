"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useDynamicId } from "@/lib/utils";

export function RedirectToListEdit({ listPath }: { listPath: string }) {
  const id = useDynamicId();
  const router = useRouter();
  useEffect(() => {
    if (id && id !== "_") {
      router.replace(`${listPath}?edit=${encodeURIComponent(id)}`);
    }
  }, [id, listPath, router]);
  return <div className="p-6 text-sm text-muted-foreground">読み込み中...</div>;
}
