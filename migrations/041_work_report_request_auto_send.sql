-- ============================================================
-- 041_work_report_request_auto_send.sql
-- 稼働報告 初回依頼メール(work_report_request)の送信タイミングを社内に自動通知
-- ============================================================
-- 案件（プロジェクト）ごとに「初回依頼を送るべき日（当月の日付）」を設定できるようにする。
-- 該当日になったら、自動でパートナーへ送るのではなく、担当者（社内）へ
-- 「そろそろ送るタイミングです」という内部通知メールを送る。
-- 理由: クロス経由で受け取った勤務表PDFを添付して送る運用のため、ファイルが
-- 手元に無い状態で自動的にパートナーへ送ってしまうと「まだ受け取っていない」という
-- 手戻りが起きる（2026-07-30、ユーザー指摘）。あくまで「送り忘れ防止」の通知に留める。
-- 二重通知防止のため、発注書側に通知済み日時を記録する。

ALTER TABLE m_project
    ADD COLUMN IF NOT EXISTS report_request_day INT;

COMMENT ON COLUMN m_project.report_request_day IS
    '稼働報告 初回依頼送信日（当月）。クライアント経由でしか稼働報告が届かない案件専用。NULLなら無効';

ALTER TABLE t_purchase_order
    ADD COLUMN IF NOT EXISTS report_request_notified_at TIMESTAMPTZ;

COMMENT ON COLUMN t_purchase_order.report_request_notified_at IS
    '稼働報告 初回依頼を送るタイミングである旨を社内へ通知した日時（二重通知防止）';
