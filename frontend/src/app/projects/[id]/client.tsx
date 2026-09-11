"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useDynamicId } from "@/lib/utils";

/** 案件詳細ページは廃止。一覧の編集モーダルへリダイレクトする。 */
export default function ProjectDetailRedirect() {
  const projectId = useDynamicId();
  const router = useRouter();

  useEffect(() => {
    if (!projectId) return;
    router.replace(`/projects?edit=${encodeURIComponent(projectId)}`);
  }, [projectId, router]);

  return (
    <div className="p-8 text-sm text-muted-foreground">
      案件一覧の編集画面へ移動しています...
    </div>
  );
}
