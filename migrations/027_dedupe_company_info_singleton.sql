-- ============================================================
-- 027_dedupe_company_info_singleton.sql — s_company_infoの重複行を統合し単一行を強制
-- ============================================================
-- s_company_info はアプリ全体で「自社情報は1件のみ」という前提で
-- `SELECT ... FROM s_company_info LIMIT 1`（ORDER BYなし）で参照されていたが、
-- ステージング・本番とも実際には2行存在していた:
--   id=1: 会社ドメイン(info@example.com)設定 ※一部環境でemail_host等が未設定
--   id=2: 個人Gmailアドレス(y.yosikawa@gmail.com)設定 ※送信元として使用不可と確認済み
-- ORDER BYがないため、どちらの行が返るかはクエリ実行のたびに不定であり、
-- 「メール送信元アドレスが時々個人アドレスになる」「SMTP未設定でメールが
-- サイレントに送信スキップされる」という2つの実害を引き起こしていた
-- （2026-07-13、ユーザー報告により発覚）。
--
-- 対応: 最も古い行(MIN(id))のみを残して他を削除し、以後重複が発生しないよう
-- 単一行を強制するユニークインデックスを追加する。
-- 認証情報（SMTPパスワード等）を含むため、残す行の内容修正自体はこの
-- migrationでは行わない（別途、環境ごとに直接更新する）。
--
-- コード側（order_repo.rs / billing_repo.rs / email_service.rs /
-- settlement_dashboard.rs の計8箇所）も `ORDER BY id LIMIT 1` に修正済み。

DELETE FROM s_company_info WHERE id <> (SELECT MIN(id) FROM s_company_info);

CREATE UNIQUE INDEX IF NOT EXISTS idx_s_company_info_singleton ON s_company_info ((true));
