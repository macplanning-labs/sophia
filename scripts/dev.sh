#!/bin/bash
# ============================================================
# Sophia 開発サーバー起動スクリプト
# ============================================================
#
# 使い方:
#   ./scripts/dev.sh           本番寄せ（nginx + Rust + Next.js HMR）※既定
#   ./scripts/dev.sh --spa     SPA一体配信（cargo run のみ・レガシー）
#   ./scripts/dev.sh stop      全プロセス / nginx 停止
#   ./scripts/dev.sh build     フロントエンド静的エクスポートのみ
#
# ポート（既定 = 本番と同じ振り分け）:
#   8111 = nginx（ブラウザの入口）
#   8119 = Rust backend
#   3000 = Next.js (HMR)
#
# --spa 時:
#   8111 = Rust（SPA + API）

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

COMPOSE_DEV=(docker compose -f docker-compose.dev.yml)

# --- stop ---
stop_all() {
    echo "[dev] 停止中..."
    pkill -f "next dev" 2>/dev/null || true
    pkill -f "target/debug/sophia" 2>/dev/null || true
    pkill -f "cargo run" 2>/dev/null || true
    "${COMPOSE_DEV[@]}" down 2>/dev/null || true
    sleep 1
    echo "[dev] 停止完了"
}

# --- build ---
build_frontend() {
    echo "[dev] フロントエンドビルド中..."
    # 動的ルート[id]/page.tsxを静的エクスポート専用の内容に一時的に書き換え
    "$SCRIPT_DIR/toggle_export_routes.sh" apply
    cd "$PROJECT_DIR/frontend"
    NEXT_OUTPUT=export npm run build 2>&1
    BUILD_STATUS=$?
    cd "$PROJECT_DIR"
    # standalone/dev用（gitのデフォルト状態）に復元
    "$SCRIPT_DIR/toggle_export_routes.sh" restore
    if [ $BUILD_STATUS -ne 0 ]; then
        echo "[dev] フロントエンドビルド失敗"
        exit 1
    fi
    echo "[dev] フロントエンドビルド完了 → frontend/out/"
}

# --- 本番寄せ: nginx + Rust + Next ---
start_prod_like() {
    stop_all

    if ! command -v docker >/dev/null 2>&1; then
        echo "[dev] Docker が必要です（nginx 用）。インストールするか ./scripts/dev.sh --spa を使ってください"
        exit 1
    fi

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Sophia 開発モード（本番寄せ: nginx）"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    echo "[1/3] nginx (port 8111)..."
    "${COMPOSE_DEV[@]}" up -d
    echo "  ✅ nginx ready"

    export PORT=8119
    # ブラウザ入口は nginx。WebAuthn / リダイレクト用
    export BASE_URL="${BASE_URL:-http://localhost:8111}"

    echo "[2/3] Rust backend (port 8119)..."
    cargo run 2>&1 &
    RUST_PID=$!

    for i in {1..60}; do
        if curl -sf http://localhost:8119/health > /dev/null 2>&1; then
            echo "  ✅ Rust backend ready"
            break
        fi
        if [ "$i" -eq 60 ]; then
            echo "  ❌ Rust backend の起動タイムアウト"
            stop_all
            exit 1
        fi
        sleep 1
    done

    echo "[3/3] Next.js frontend (port 3000)..."
    cd "$PROJECT_DIR/frontend"
    # Next の rewrite 先（直接 3000 に当たった場合の保険）。入口は nginx。
    export NEXT_PUBLIC_API_BASE_URL="${NEXT_PUBLIC_API_BASE_URL:-http://localhost:8119}"
    npm run dev -- --port 3000 2>&1 &
    NEXT_PID=$!
    cd "$PROJECT_DIR"

    for i in {1..60}; do
        if curl -sf http://localhost:3000 > /dev/null 2>&1; then
            echo "  ✅ Next.js frontend ready"
            break
        fi
        if [ "$i" -eq 60 ]; then
            echo "  ❌ Next.js の起動タイムアウト"
            stop_all
            exit 1
        fi
        sleep 1
    done

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  🚀 http://localhost:8111  （nginx → Next / Rust）"
    echo "     内部: Next :3000 / Rust :8119"
    echo "  Ctrl+C で停止"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    trap "stop_all" INT TERM
    wait
}

# --- コマンド分岐 ---
case "${1:-}" in
    stop)
        stop_all
        exit 0
        ;;
    build)
        build_frontend
        exit 0
        ;;
    --spa)
        # レガシー: SPA一体配信（nginx なし）
        stop_all

        if [ ! -d "$PROJECT_DIR/frontend/out" ]; then
            build_frontend
        else
            echo "[dev] frontend/out/ は既にビルド済み（再ビルドは ./scripts/dev.sh build）"
        fi

        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo "  Sophia SPA一体配信モード（レガシー）"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

        echo "[dev] cargo run (port 8111)..."
        cargo run 2>&1 &
        RUST_PID=$!

        for i in {1..30}; do
            if curl -sf http://localhost:8111/health > /dev/null 2>&1; then
                echo "  ✅ Rust server ready"
                break
            fi
            sleep 1
        done

        echo ""
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo "  🚀 http://localhost:8111 (SPA一体配信)"
        echo "  Ctrl+C で停止"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

        trap "stop_all" INT TERM
        wait
        ;;
    --hmr)
        # 互換: 旧 --hmr は本番寄せ（nginx 付き）に統合
        echo "[dev] --hmr は既定モードに統合されました（nginx + HMR）"
        start_prod_like
        ;;
    "")
        start_prod_like
        ;;
    *)
        echo "使い方: ./scripts/dev.sh [--spa|--hmr|stop|build]"
        exit 1
        ;;
esac
