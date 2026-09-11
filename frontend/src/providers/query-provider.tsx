"use client";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { toast } from "sonner";
import { SessionExpiredError } from "@/lib/session-expired";

export function QueryProvider({ children }: { children: ReactNode }) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            staleTime: 30 * 1000, // 30秒間キャッシュ有効
            gcTime: 5 * 60 * 1000, // 5分GC
            refetchOnWindowFocus: false,
            retry: 1,
          },
          mutations: {
            onError: (error: Error) => {
              // セッション切れはモーダルで案内済みなので、重複するエラートーストは出さない
              if (error instanceof SessionExpiredError) return;
              toast.error(`処理に失敗しました: ${error.message}`);
            },
          },
        },
      })
  );

  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}
