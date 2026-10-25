/// domain/services/email_test.rs — メール送信テストツール
///
/// `sophia test-email --to user@example.com --template order_publish`
/// でテンプレートの変数置換とSMTP送信を検証する。

use std::collections::{BTreeSet, HashMap};

use anyhow::Result;
use sqlx::PgPool;
use crate::domain::services::email_service::EmailService;
use crate::infrastructure::repositories::{email_template_repo, order_repo, partner_repo};

/// テンプレートが使う変数名（migrations の件名・本文にある `{{ x }}` と一致させる）
const TEMPLATE_VARS: &[&str] = &[
    "accepted_at", "action_phrase", "client_name", "client_shared_url", "company_name", "company_tel",
    "days_pending", "deadline_date", "deadline_display", "deadline_message", "display_name", "due_date",
    "engineer_name", "engineer_names", "expiry_days", "invite_url", "invoice_id", "login_url", "month",
    "month_display", "month_short", "notice_id", "order_id", "order_url", "partner_name", "project_name",
    "report_lines", "subject_urgency", "target_month", "token_url", "total", "total_amount", "username",
    "work_end", "work_start", "year_month", "ym_str",
];

/// テスト送信用の変数を作る（DB に触れない）。URL は base_url から組み立てる。
/// 宛先（email）と契約 ID（contract_url）は、呼び出し側が実値で差し替える。
pub fn test_context(base_url: &str, order_id: &str, order_uuid: &str) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("company_name".into(), "【テスト】有限会社マックプランニング".into());
    ctx.insert("company_tel".into(), "03-0000-0000".into());
    ctx.insert("partner_name".into(), "テストパートナー株式会社".into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("order_date".into(), "2026-07-05".into());
    ctx.insert("project_name".into(), "テストプロジェクト".into());
    ctx.insert("work_start".into(), "2026-07-01".into());
    ctx.insert("work_end".into(), "2026-07-31".into());
    ctx.insert("order_url".into(), format!("{}/orders/{}", base_url, order_id));
    ctx.insert("token_url".into(), format!("{}/token/{}", base_url, order_uuid));
    ctx.insert("login_url".into(), format!("{}/login", base_url));
    ctx.insert("invoice_no".into(), "INV-TEST-000001".into());
    ctx.insert("invoice_id".into(), "INV-TEST-000001".into());
    ctx.insert("invoice_url".into(), format!("{}/invoices/INV-TEST-000001", base_url));
    ctx.insert("notice_id".into(), "PN-TEST-000001".into());
    ctx.insert("target_month".into(), "2026年07月".into());
    ctx.insert("month".into(), "2026年07月".into());
    ctx.insert("month_short".into(), "2026/07".into());
    ctx.insert("total_amount".into(), "550,000".into());
    ctx.insert("total".into(), "550,000".into());
    ctx.insert("payment_deadline".into(), "2026-08-31".into());
    ctx.insert("due_date".into(), "2026-08-31".into());
    ctx.insert("client_name".into(), "テストクライアント株式会社".into());
    ctx.insert("contact_person".into(), "担当太郎".into());
    ctx.insert("display_name".into(), "テスト太郎".into());
    ctx.insert("month_display".into(), "2026年07月".into());
    ctx.insert("engineer_name".into(), "テスト太郎".into());
    ctx.insert("engineer_names".into(), "テスト太郎".into());
    ctx.insert("ym_str".into(), "2026年07月".into());
    ctx.insert("target_month_str".into(), "2026年07月".into());
    ctx.insert("year_month".into(), "2026年07月".into());
    ctx.insert("deadline".into(), "2026年07月05日".into());
    ctx.insert("deadline_date".into(), "2026-07-05".into());
    ctx.insert("deadline_display".into(), "2026年07月05日（日）".into());
    ctx.insert("deadline_message".into(), "【テスト】提出期限は2026年07月05日です。".into());
    ctx.insert("subject_urgency".into(), "【テスト】".into());
    ctx.insert("action_phrase".into(), "【テスト】".into());
    ctx.insert("accepted_at".into(), "2026-07-05 12:00:00".into());
    ctx.insert("expiry_days".into(), "7".into());
    ctx.insert("days_pending".into(), "5".into());
    ctx.insert("address".into(), "東京都千代田区テスト1-1-1".into());
    ctx.insert("representative_name".into(), "代表太郎".into());
    ctx.insert("registration_no".into(), "T1234567890123".into());
    ctx.insert("progress_url".into(), format!("{}/partner-contracts", base_url));
    ctx.insert("contract_url".into(), format!("{}/partner-contracts/1", base_url));
    ctx.insert("signed_at".into(), "2026-07-05 12:00:00".into());
    ctx.insert("signed_by".into(), "テスト承認者".into());
    ctx.insert("invite_url".into(), format!("{}/token/{}", base_url, order_uuid));
    ctx.insert("email".into(), "test@example.invalid".into());
    ctx.insert("password".into(), "test-password-123".into());
    ctx.insert("work_report_url".into(), format!("{}/timesheets", base_url));
    ctx.insert("customer_name".into(), "テスト顧客株式会社".into());
    ctx.insert("username".into(), "管理者".into());
    ctx.insert("report_lines".into(), "  ・テスト太郎: 160.0h / 20日".into());
    ctx.insert("client_shared_url".into(), format!("{}/shared/test", base_url));
    ctx
}

/// テストメール送信
pub async fn run_email_test(pool: &PgPool, to: &str, template_code: Option<&str>) -> Result<()> {
    // テンプレート一覧を表示
    let all_templates = email_template_repo::list_all_templates(pool).await?;
    let templates: Vec<(String, String, String)> = all_templates
        .into_iter()
        .map(|t| (t.code, t.subject, String::new()))
        .collect();

    if templates.is_empty() {
        tracing::error!("❌ テンプレートが1件もありません");
        return Ok(());
    }

    match template_code {
        None => {
            // テンプレート一覧を表示
            tracing::info!("📋 利用可能なテンプレート:");
            for (code, subject, desc) in &templates {
                tracing::info!("  {} — {} ({})", code, subject, desc);
            }
            tracing::info!("\n--template <code> で指定してテスト送信できます");
        }
        Some(code) => {
            // BASE_URLから実URLを生成
            let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:8110".into());

            // DBから実データを取得してテストURLを構築
            let order_uuid = order_repo::find_latest_purchase_order_uuid(pool)
                .await?
                .unwrap_or_else(|| "no-order-found".into());

            let order_id = order_repo::find_latest_purchase_order_id(pool)
                .await?
                .unwrap_or_else(|| "SP-TEST-000001".into());

            // partner_contract_id（テストには必須ではないので、取得失敗時はデフォルト値使用）
            let partner_contract_id = partner_repo::find_latest_partner_contract_id(pool)
                .await
                .ok()
                .flatten()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "1".into());

            // テスト用コンテキストを構築（実URL使用）
            let mut ctx = test_context(&base_url, &order_id, &order_uuid);
            ctx.insert("contract_url".into(), format!("{}/partner-contracts/{}", base_url, partner_contract_id));
            ctx.insert("email".into(), to.into());

            tracing::info!("📧 テストメール送信: テンプレート={}, 宛先={}", code, to);

            let email_svc = EmailService::new(pool.clone());
            match email_svc.send_by_template(code, to, None, &ctx).await {
                Ok(()) => {
                    tracing::info!("✅ テストメール送信成功！ {} に {} を送信しました", to, code);
                }
                Err(e) => {
                    tracing::error!("❌ テストメール送信失敗: {:?}", e);
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

/// `{{ name }}` の変数名を集める
fn placeholders(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = src;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !name.is_empty() {
            out.insert(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テンプレートを登録している migrations（件名・本文の INSERT / UPDATE を含むもの）
    const TEMPLATE_MIGRATIONS: &[&str] = &[
        include_str!("../../../migrations/001_init.sql"),
        include_str!("../../../migrations/003_seed_email_templates.sql"),
        include_str!("../../../migrations/015_seed_invite_template.sql"),
        include_str!("../../../migrations/016_portal_auth.sql"),
        include_str!("../../../migrations/022_client_invoice_send_template.sql"),
        include_str!("../../../migrations/023_seed_missing_email_templates.sql"),
        include_str!("../../../migrations/024_fix_order_send_expiry_text.sql"),
        include_str!("../../../migrations/025_fix_work_report_share_template.sql"),
        include_str!("../../../migrations/026_fix_reminder_templates_and_cleanup.sql"),
        include_str!("../../../migrations/035_payment_notice_document_hash_and_notice_send.sql"),
        include_str!("../../../migrations/036_work_report_reminder_template.sql"),
        include_str!("../../../migrations/037_notice_send_template_rewording.sql"),
        include_str!("../../../migrations/038_order_send_template_rewording.sql"),
        include_str!("../../../migrations/039_work_report_reminder_rewording.sql"),
        include_str!("../../../migrations/040_work_report_request_template.sql"),
        include_str!("../../../migrations/042_work_report_request_internal_notify_template.sql"),
        include_str!("../../../migrations/043_client_invoice_token_send.sql"),
        include_str!("../../../migrations/052_client_invoice_receipt_confirmed_template.sql"),
    ];

    fn ctx() -> HashMap<String, String> {
        test_context("http://localhost:8110", "SP-TEST-000001", "00000000-0000-0000-0000-000000000000")
    }

    #[test]
    fn test_context_provides_every_template_variable() {
        let ctx = ctx();
        let missing: Vec<&str> = TEMPLATE_VARS.iter().copied().filter(|v| !ctx.contains_key(*v)).collect();
        assert!(missing.is_empty(), "ctx に無い変数: {missing:?}");
    }

    #[test]
    fn every_placeholder_in_migration_templates_is_provided() {
        let ctx = ctx();
        let mut missing = BTreeSet::new();
        for src in TEMPLATE_MIGRATIONS {
            for name in placeholders(src) {
                if !ctx.contains_key(&name) {
                    missing.insert(name);
                }
            }
        }
        assert!(missing.is_empty(), "テスト送信の ctx に無い変数: {missing:?}");
    }

    #[test]
    fn placeholders_ignore_spacing() {
        let names = placeholders("a {{x}} b {{ y }} c {{  }} d {{ z");
        assert_eq!(names.into_iter().collect::<Vec<_>>(), vec!["x".to_string(), "y".to_string()]);
    }
}
