-- ============================================================
-- 023_seed_missing_email_templates.sql — 未登録メールテンプレートの追加
-- ============================================================
-- 以下6コードはRust側(email_service.rs)から参照されているが、s_email_templateに
-- 一度も登録されておらず、送信時に「メールテンプレートが見つかりません」で
-- 失敗していた（2026-07-13、発注書送付プレビュー機能追加で order_publish の
-- 失敗がUIに表面化して判明。他5件は send_by_template の fire-and-forget実装のため
-- これまでエラーログのみで静かに失敗し続けていた）。
--
-- order_publish/order_approve/order_approve_reminder/invoice_approve は、削除済み
-- 旧Django(EDI_MP_1)の core/domain/services/email_service.py に実在したデフォルト
-- 文面（gitタグ `pre-django-cleanup` で参照可能）をベースに、Rust版が実際に渡す
-- context変数に合わせて復元。
-- order_republish/report_approved はEDI_MPに前例がなくRust移植時の新規機能のため、
-- 他テンプレートの文体を踏襲して新規ドラフト作成（要レビュー）。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('order_publish',
     '【{{ company_name }}】注文書送付のご連絡（注文番号：{{ order_id }}）',
     E'{{ partner_name }} 様\n\nいつもお世話になっております。\n{{ company_name }}でございます。\n\n注文書を登録いたしましたので、下記URLよりご確認ください。\n\n▼ 注文書確認・承諾URL\n{{ token_url }}\n\n《送付物》\n「注文書」「注文請書」各1通\n\n《お願い》\n注文書内容をご確認いただき、内容にご同意いただける場合は「承諾」ボタンを押してください。\n承諾いただけない場合はメールにてご返信ください。\n※「承諾」をいただいた場合は、注文請書のご返送は不要となります。\n\n■注文番号：{{ order_id }}\n■プロジェクト：{{ project_name }}\n■作業期間：{{ work_start }} 〜 {{ work_end }}\n\n以上、よろしくお願いいたします。\n\n--------------------------------------------------\n{{ company_name }}\nTEL: {{ company_tel }}\n--------------------------------------------------',
     '注文書送付メール（パートナー宛。EDI_MP order_publish を復元、宛名を追加）'),

    ('order_approve',
     '【承認通知】{{ partner_name }}様 注文番号：{{ order_id }}',
     E'{{ partner_name }} 様より、以下の注文書が承諾されました。\n\n■注文番号：{{ order_id }}\n■プロジェクト：{{ project_name }}\n\nご確認ください。',
     '注文書承諾通知メール（社内向け。EDI_MP order_approve を復元）'),

    ('order_approve_reminder',
     '【{{ company_name }}】注文書ご承諾のお願い（注文番号：{{ order_id }}）',
     E'{{ partner_name }}　御中\n\nいつもお世話になっております。\n{{ company_name }}でございます。\n\n注文書をお送りしておりますが、まだご承諾の確認がとれておりません（送付から{{ days_pending }}日経過）。\n\n■ 注文番号：{{ order_id }}\n\n内容にご同意いただける場合は、下記URLより「承諾」ボタンを押してください。\n\n▼ 注文書確認・承諾URL\n{{ token_url }}\n\n--------------------------------------------------\n{{ company_name }}\nTEL: {{ company_tel }}\n--------------------------------------------------',
     '注文書承諾リマインドメール（パートナー宛。EDI_MP order_approve_reminder をベースにdays_pendingを反映）'),

    ('invoice_approve',
     '【支払通知書承諾通知】{{ invoice_id }}',
     E'{{ partner_name }} 様より、以下の支払通知書が承諾されました。\n\n■通知書番号：{{ invoice_id }}\n■対象年月：{{ target_month }}\n■税込合計：¥{{ total_amount }}\n\nご確認ください。',
     '支払通知書承諾通知メール（社内向け。EDI_MP invoice_approve をベースにSophiaのt_payment_notice向けへ調整）'),

    ('order_republish',
     '【{{ company_name }}】注文書訂正のご連絡（注文番号：{{ order_id }}）',
     E'{{ partner_name }} 様\n\nいつもお世話になっております。\n{{ company_name }}でございます。\n\n先にお送りした注文書の内容に訂正がございましたので、訂正版を登録いたしました。\nお手数ですが、下記URLより改めて内容をご確認の上、ご承諾をお願いいたします。\n\n▼ 注文書確認・承諾URL\n{{ order_url }}\n\n■注文番号：{{ order_id }}\n\nご不明な点がございましたら、本メールにご返信ください。\n\n以上、よろしくお願いいたします。\n\n--------------------------------------------------\n{{ company_name }}\nTEL: {{ company_tel }}\n--------------------------------------------------',
     '注文書訂正再送付メール（パートナー宛。EDI_MPに前例なし、新規ドラフト・要レビュー）'),

    ('report_approved',
     '【稼働報告確定】{{ month_display }} {{ display_name }}',
     E'{{ display_name }} さんの {{ month_display }} 分稼働報告が確定しました。\n（確定操作: {{ username }}）\n\n{{ report_lines }}\n\nご確認ください。',
     '稼働報告確定通知メール（社内向け。EDI_MPに前例なし、新規ドラフト・要レビュー）')
ON CONFLICT (code) DO NOTHING;
