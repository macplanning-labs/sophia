import type { Metadata } from "next";
import "./globals.css";
import { Sidebar } from "@/components/layout/sidebar";
import { AppHeader } from "@/components/layout/app-header";
import { QueryProvider } from "@/providers/query-provider";
import { Toaster } from "@/components/ui/sonner";
import { MfaSetupGate } from "@/components/mfa-setup-gate";
import { SessionExpiredModal } from "@/components/session-expired-modal";

export const metadata: Metadata = {
  title: "Sophia | SES受発注管理",
  description: "SES事業の受発注・稼働報告・給与計算を統合管理",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="ja" className="dark" suppressHydrationWarning>
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
        <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet" />
      </head>
      <body className="font-sans antialiased bg-background text-foreground min-h-screen" style={{ fontFamily: "'Inter', sans-serif" }}>
        <QueryProvider>
          <div className="flex min-h-screen">
            <Sidebar />
            <div className="flex-1 flex flex-col min-w-0">
              <AppHeader />
              <main className="flex-1 overflow-auto">
                <MfaSetupGate>{children}</MfaSetupGate>
              </main>
            </div>
          </div>
          <Toaster richColors position="top-right" />
          <SessionExpiredModal />
        </QueryProvider>
      </body>
    </html>
  );
}
