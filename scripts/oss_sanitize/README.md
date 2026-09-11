# OSS sanitize（作業コピー専用）

## 目的
公開候補ツリーから社内ホスト・内部 IP・既定認証表記・平文寄りの接続文字列を機械除去する。
**実 PII / 秘密の値はログに出さない。** ダミーシードは公開リポジトリに同梱しない。

## 使い方
```bash
# 作業コピー根で
python3 scripts/oss_sanitize/sanitize_tree.py --dry-run
python3 scripts/oss_sanitize/sanitize_tree.py
```

## 手元での追加 PII 除去
1. DB ダンプや `uploads/` / `media/` を公開ツリーに置かない
2. 社員名・連絡先・口座が含まれる SQL／CSV は削除（本ディレクトリにシードを追加しない）
3. 実行後は `gitleaks detect --log` で履歴も確認（Cycle 4 T8）

## 禁止
- Sophia 本体パスでの実行・commit
- 置換前後の秘密値のチャット／チケット貼付
