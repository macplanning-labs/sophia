import type { NextConfig } from "next";

// NEXT_OUTPUT=export → 静的エクスポート (cargo run 一本化)
// NEXT_OUTPUT=standalone → Docker 本番用
// 未設定 → 開発モード (npm run dev + rewrites)
const isExport = process.env.NEXT_OUTPUT === "export";
const isStandalone = process.env.NEXT_OUTPUT === "standalone";

const nextConfig: NextConfig = {
  output: isExport ? "export" : isStandalone ? "standalone" : undefined,
  trailingSlash: isExport,
  images: {
    unoptimized: isExport,
  },
  async redirects() {
    return [
      // 旧 MFA 検証 URL → Next.js 画面
      { source: "/mfa/verify", destination: "/mfa", permanent: true },
      // 旧トークン PDF 直リンク → §14 API（nginx 経由で Next に落ちた場合の互換）
      { source: "/token/:uuid/pdf", destination: "/api/v1/token/:uuid/pdf", permanent: false },
      { source: "/token/:uuid/invoice-pdf", destination: "/api/v1/token/:uuid/invoice-pdf", permanent: false },
      { source: "/token/:uuid/acceptance-pdf", destination: "/api/v1/token/:uuid/acceptance-pdf", permanent: false },
    ];
  },
  // 開発時のみ API プロキシ (npm run dev)
  ...(!isExport && !isStandalone
    ? {
        async rewrites() {
          const apiUrl =
            process.env.NEXT_PUBLIC_API_BASE_URL || "http://localhost:8119";
          return [
            {
              source: "/api/:path*",
              destination: `${apiUrl}/api/:path*`,
            },
          ];
        },
      }
    : {}),
};

export default nextConfig;
