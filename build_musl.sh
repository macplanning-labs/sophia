#!/bin/bash
# ============================================================
# build_musl.sh — NAS用クロスコンパイルビルドスクリプト
# ============================================================
# 前提: brew install filosottile/musl-cross/musl-cross
# ============================================================

set -euo pipefail

echo "=== Sophia NAS用クロスコンパイル ==="

# musl-cross 確認
if ! command -v x86_64-linux-musl-gcc &> /dev/null; then
    echo "❌ x86_64-linux-musl-gcc が見つかりません"
    echo "   brew install filosottile/musl-cross/musl-cross"
    exit 1
fi

# ビルド
echo ">>> cargo build --release --target x86_64-unknown-linux-musl ..."
cargo build --release --target x86_64-unknown-linux-musl

# 結果表示
BINARY="target/x86_64-unknown-linux-musl/release/sophia"
if [ -f "$BINARY" ]; then
    SIZE=$(ls -lh "$BINARY" | awk '{print $5}')
    echo "✅ ビルド成功: $BINARY ($SIZE)"
    file "$BINARY"
else
    echo "❌ ビルド失敗"
    exit 1
fi
