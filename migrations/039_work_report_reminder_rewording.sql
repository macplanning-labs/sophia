-- ============================================================
-- 039_work_report_reminder_rewording.sql
-- work_report_reminder（稼働報告 自動催促メール）の文面を刷新
-- ============================================================
-- 背景: これまでは「{{ month }} 分の稼働報告書がまだ届いておりません」の一律文面だったが、
-- ユーザーから「期限との位置関係（期限前／当日／超過後）で言い回しを変えたい」
-- 「複数エンジニアがいる場合は1通に名前を列挙したい」との要望を受けて改訂。
-- 実際の文面出し分けは compose_work_report_reminder_email() 側で行い、
-- テンプレートは {{ deadline_message }} 等の完成済み変数を差し込むだけのシンプルな形にする。
--
-- 変数:
--   {{ subject_urgency }} … 「【至急】」「【本日締切】」「【ご登録のお願い】」のいずれか
--   {{ month_short }}     … 件名用の短い月表記（例: 07月）
--   {{ deadline_message }} … 期限との位置関係に応じた冒頭文（完成済み1文）
--   {{ engineer_names }}   … 未登録エンジニア名（さん付け、複数は「、」区切り）
--   {{ deadline_display }} … 「07月15日（水）」形式の期限表示
--   {{ token_url }}        … 稼働報告アップロードページURL
--   {{ action_phrase }}    … 「至急」「本日中に」「期限までに」のいずれか

UPDATE s_email_template
SET subject = '{{ subject_urgency }}{{ month_short }}分の勤務表について',
    body = E'{{ deadline_message }}\n{{ engineer_names }}のデータが未登録のようでしたので、念のためご連絡いたしました。\n\n■提出期限：{{ deadline_display }}\n■システムURL：{{ token_url }}\n\n業務中にお手数をおかけしますが、{{ action_phrase }}ご対応いただけますと幸いです。\nもし、本メールと行き違いで既に登録いただいている場合は、何卒ご容赦ください。',
    description = '稼働報告提出の自動催促メール（自社→パートナー。期限前／当日／超過後で文面を出し分け）',
    updated_at = NOW()
WHERE code = 'work_report_reminder';
