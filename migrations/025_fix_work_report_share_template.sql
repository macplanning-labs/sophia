-- ============================================================
-- 025_fix_work_report_share_template.sql — WORK_REPORT_SHAREテンプレートの追加/修正
-- ============================================================
-- received_orders/action.rsが小文字"work_report_share"でsend_by_templateを呼んで
-- おり、DBの行(WORK_REPORT_SHARE、staging限定・どのmigrationにも属さない手動投入
-- データ)と大文字小文字が一致せず常に送信失敗していた。コード側をWORK_REPORT_SHARE
-- に修正した上で、本テンプレートを正式にmigration管理下へ移す。
--
-- また、staging上の既存本文は{{ partner_name }}/{{ ym_str }}/{{ client_shared_url }}
-- を参照していたが、compose_work_report_share_email()が実際に渡すcontextは
-- client_name/year_month/engineer_nameのみで、変数名が一致していなかった
-- （client_shared_urlは現状どこからも渡されない）。実際のcontextに合わせて修正。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('WORK_REPORT_SHARE',
     '【稼働報告書】{{ year_month }}分 稼働報告のご連絡',
     E'{{ client_name }} 様\n\nいつもお世話になっております。\n{{ engineer_name }} の {{ year_month }} 分の稼働報告を受領いたしましたのでご連絡いたします。\n\n※本メールはシステムより自動送信されています。',
     '稼働報告受領のクライアント通知メール（自社→クライアント）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
