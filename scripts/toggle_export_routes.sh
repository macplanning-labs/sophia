#!/bin/bash
# ============================================================
# toggle_export_routes.sh — 動的ルート[id]/page.tsxを
# 静的エクスポート(NEXT_OUTPUT=export)専用の内容に一時的に書き換える/元に戻す
# ============================================================
#
# 背景:
#   Next.jsの `dynamicParams` はリテラルの true/false のみ許可され、
#   環境変数による条件式にできない(Turbopackの静的解析の制約)。
#   このプロジェクトはビルドモードが2つある:
#     - standalone/dev（Docker本番・ステージング・npm run dev）
#       → dynamicParams=true で実IDを動的レンダリング（gitのデフォルト状態）
#     - export（cargo run 一本化のローカル開発、./scripts/dev.sh）
#       → 全ID事前生成は不可能なため "_" プレースホルダーのみ生成し、
#         Rust側(routes.rs)のフォールバックとクライアント側(useDynamicId)で解決
#   このスクリプトは export ビルド前に一時的に書き換え、ビルド後に
#   `git checkout` でリポジトリのデフォルト(standalone/dev用)に戻す。
#
# 使い方:
#   scripts/toggle_export_routes.sh apply    # exportモード用に書き換え
#   scripts/toggle_export_routes.sh restore  # git checkoutで元に戻す
#
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_DIR="$PROJECT_DIR/frontend/src/app"

# パス:コンポーネント名[:パラメータ名]（パラメータ名省略時は"id"。パスは複数階層可、[param]は含めない）
ROUTES=(
  "users:UserDetailPage"
  "timesheets:TimesheetDetailPage"
  "received-orders:ReceivedOrderDetailPage"
  "payroll:PayrollDetailPage"
  "partner-contracts:PartnerContractDetailPage"
  "orders:OrderDetailPage"
  "notices:NoticeDetailPage"
  "invoices:InvoiceDetailPage"
  "expenses:ExpenseDetailPage"
  "employees:EmployeeDetailPage"
  "client-contracts:ClientContractDetailPage"
  "projects:ProjectDetailPage"
  "upload/mobile:MobileUploadPage:token"
  "token:TokenPage:uuid"
  "invite:InvitePage:uuid"
  "portal/auth:PortalAuthPage:token"
)

apply() {
  echo "[toggle_export_routes] 静的エクスポート用に書き換え中..."
  for entry in "${ROUTES[@]}"; do
    IFS=':' read -r dir component param <<< "$entry"
    param="${param:-id}"
    file="$APP_DIR/$dir/[$param]/page.tsx"
    cat > "$file" <<EOF
// Server Component wrapper (静的エクスポート専用・scripts/toggle_export_routes.shが自動生成)
// ビルド後は 'scripts/toggle_export_routes.sh restore' で元に戻すこと
import ${component} from "./client";

export const dynamicParams = false;

export function generateStaticParams() {
  return [{ ${param}: "_" }];
}

export default function Page() {
  return <${component} />;
}
EOF
  done
  echo "[toggle_export_routes] 完了（${#ROUTES[@]}ファイル）"
}

restore() {
  echo "[toggle_export_routes] standalone/dev用に復元中..."
  cd "$PROJECT_DIR"
  for entry in "${ROUTES[@]}"; do
    IFS=':' read -r dir component param <<< "$entry"
    param="${param:-id}"
    git checkout -- "frontend/src/app/$dir/[$param]/page.tsx"
  done
  echo "[toggle_export_routes] 完了"
}

case "${1:-}" in
  apply) apply ;;
  restore) restore ;;
  *)
    echo "使い方: $0 apply|restore"
    exit 1
    ;;
esac
