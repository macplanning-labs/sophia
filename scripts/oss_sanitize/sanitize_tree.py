#!/usr/bin/env python3
"""OSS 公開向けツリーサニタイズ（作業コピー専用）。

- 実 PII / 秘密の値は標準出力・ログに出さない（件数とパス種別のみ）
- 本体 Sophia では実行しないこと
- 公開リポジトリにダミーシードは同梱しない（置換手段のみ）
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SKIP_DIR_NAMES = {
    ".git",
    "target",
    "node_modules",
    "out",
    "tmp",
    "uploads",
    "media",
    ".cargo",
}
TEXT_SUFFIXES = {
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".md",
    ".yml",
    ".yaml",
    ".sql",
    ".toml",
    ".json",
    ".sh",
    ".txt",
    ".html",
    ".inc",
    ".conf",
    ".mjs",
    ".mts",
    ".css",
    ".py",
    ".example",
    ".template",
}

# 置換ルール（値はプレースホルダのみ。実シークレットをここに書かない）
REPLACEMENTS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"\b192\.168\.\d{1,3}\.\d{1,3}\b"), "203.0.113.10"),
    (re.compile(r"\b10\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"), "203.0.113.10"),
    (re.compile(r"sophia\.macplanning\.com"), "sophia.example.com"),
    (re.compile(r"sophia-stg\.macplanning\.com"), "sophia-stg.example.com"),
    (re.compile(r"edi\.macplanning\.com"), "edi.example.com"),
    (re.compile(r"https://github\.com/amagi019/Sophia\b"), "https://github.com/macplanning-labs/sophia"),
    (re.compile(r"macplanning\.com"), "example.com"),
    (re.compile(r"macplanning\.local"), "example.local"),
    (re.compile(r"admin@sophia\.local"), "admin@example.com"),
    (re.compile(r"@[a-z0-9.-]*sophia\.local\b"), "@example.com"),
    (re.compile(r"POSTGRES_PASSWORD:\s*sophia\b"), "POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}"),
    (
        re.compile(
            r"DATABASE_URL:\s*postgresql://sophia:sophia@db:5432/sophia"
        ),
        "DATABASE_URL: ${DATABASE_URL}",
    ),
    (
        re.compile(
            r"postgresql://sophia:sophia@"
        ),
        "postgresql://sophia:${POSTGRES_PASSWORD}@",
    ),
    (re.compile(r"PGPASSWORD=sophia\b"), "PGPASSWORD=${POSTGRES_PASSWORD}"),
    (re.compile(r"-e POSTGRES_PASSWORD=sophia\b"), "-e POSTGRES_PASSWORD=${POSTGRES_PASSWORD}"),
    (re.compile(r"0\d{1,4}-\d{1,4}-\d{3,4}"), "03-0000-0000"),
]


def should_skip(path: Path) -> bool:
    return any(part in SKIP_DIR_NAMES for part in path.parts)


def is_text_file(path: Path) -> bool:
    name = path.name
    if name == "sanitize_tree.py":
        # このスクリプト自身は置換ルールの文字列表現(平文パスワード等)をソース中に
        # 持つため、自己走査するとルール自体が書き換わって壊れる。常に除外する。
        return False
    if name.startswith(".env"):
        return True
    return path.suffix.lower() in TEXT_SUFFIXES


def sanitize_text(text: str) -> tuple[str, int]:
    hits = 0
    out = text
    for pat, repl in REPLACEMENTS:
        out, n = pat.subn(repl, out)
        hits += n
    return out, hits


def main() -> int:
    ap = argparse.ArgumentParser(description="Sanitize tree for OSS extract (no secret logging)")
    ap.add_argument("--root", type=Path, default=Path.cwd())
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    root: Path = args.root.resolve()

    files_changed = 0
    total_hits = 0
    by_kind: dict[str, int] = {}

    for path in root.rglob("*"):
        if not path.is_file() or should_skip(path.relative_to(root)):
            continue
        if not is_text_file(path):
            continue
        try:
            original = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        new, hits = sanitize_text(original)
        if hits == 0:
            continue
        total_hits += hits
        files_changed += 1
        kind = path.relative_to(root).parts[0] if path.relative_to(root).parts else str(path)
        by_kind[kind] = by_kind.get(kind, 0) + hits
        if not args.dry_run:
            path.write_text(new, encoding="utf-8")

    print(f"mode={'dry-run' if args.dry_run else 'apply'}")
    print(f"files_changed={files_changed}")
    print(f"replacement_hits={total_hits}")
    for kind, n in sorted(by_kind.items(), key=lambda x: (-x[1], x[0])):
        print(f"kind\t{kind}\t{n}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
