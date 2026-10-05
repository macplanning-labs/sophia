import type { Metadata } from "next";
import "./globals.css";
import { Sidebar } from "@/components/layout/sidebar";
import { AppHeader } from "@/components/layout/app-header";
import { QueryProvider } from "@/providers/query-provider";
import { MonthProvider } from "@/lib/MonthContext";
import { CommandPaletteProvider } from "@/lib/CommandPaletteContext";
import { CommandPalette } from "@/components/layout/command-palette";
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
      <body className="font-sans antialiased bg-background text-foreground h-screen overflow-hidden" style={{ fontFamily: "'Inter', sans-serif" }}>
        <QueryProvider>
          <MonthProvider>
            <CommandPaletteProvider>
              <div className="flex h-screen">
                <Sidebar />
                <div className="flex-1 flex flex-col min-w-0 min-h-0">
                  <AppHeader />
                  <main className="flex-1 min-h-0 overflow-auto">
                    <MfaSetupGate>{children}</MfaSetupGate>
                  </main>
                </div>
              </div>
              <CommandPalette />
              <Toaster richColors position="top-right" />
              <SessionExpiredModal />
            </CommandPaletteProvider>
          </MonthProvider>
        </QueryProvider>
      </body>
    </html>
  );
}
