-- 過去の 取引先の EDI 通知メールを、すべて対象外にする（2026-10-05 障害の対策）
--
-- これまで Phase2 は、通知メールの有無に関わらず 15分毎に 取引先の EDI へログインしていた。
-- 今後は「未処理の通知メール（source_type='EDI_API' かつ NEW / FETCH_FAILED）」がある時だけ接続する。
-- 過去の通知メールは取込の対象外（現在 取引先の EDI 経由の取引は無い）のため、ここで一括して SKIPPED にし、
-- この変更の反映直後に過去分をきっかけとした接続が起きないようにする。
-- Phase2 が合成する行（edi-order: / edi-invoice:）は FETCHED 以降の状態なので対象にならないが、念のため除外する。
UPDATE t_received_email
SET status        = 'SKIPPED',
    processed_at  = COALESCE(processed_at, NOW()),
    error_message = '過去の取引先の EDI通知メールのため対象外（2026-10-05 一括）'
WHERE source_type = 'EDI_API'
  AND status IN ('NEW', 'FETCH_FAILED')
  AND message_id NOT LIKE 'edi-order:%'
  AND message_id NOT LIKE 'edi-invoice:%';
