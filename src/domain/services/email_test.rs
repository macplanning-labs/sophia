/// domain/services/email_test.rs — メール送信テストツール
///
/// `sophia test-email --to user@example.com --template order_publish`
/// でテンプレートの変数置換とSMTP送信を検証する。

use anyhow::Result;
use sqlx::PgPool;
use crate::domain::services::email_service::EmailService;
use crate::infrastructure::repositories::{email_template_repo, order_repo, partner_repo};

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
            let mut ctx = std::collections::HashMap::new();
            ctx.insert("company_name".into(), "【テスト】有限会社マックプランニング".into());
            ctx.insert("company_tel".into(), "03-0000-0000".into());
            ctx.insert("partner_name".into(), "テストパートナー株式会社".into());
            ctx.insert("order_id".into(), order_id.clone());
            ctx.insert("order_date".into(), "2026-07-05".into());
            ctx.insert("project_name".into(), "テストプロジェクト".into());
            ctx.insert("work_start".into(), "2026-07-01".into());
            ctx.insert("work_end".into(), "2026-07-31".into());
            ctx.insert("order_url".into(), format!("{}/orders/{}", base_url, order_id));
            ctx.insert("token_url".into(), format!("{}/token/{}", base_url, order_uuid));
            ctx.insert("login_url".into(), format!("{}/login", base_url));
            ctx.insert("invoice_no".into(), "INV-TEST-000001".into());
            ctx.insert("invoice_url".into(), format!("{}/invoices/INV-TEST-000001", base_url));
            ctx.insert("notice_id".into(), "PN-TEST-000001".into());
            ctx.insert("target_month".into(), "2026年07月".into());
            ctx.insert("total_amount".into(), "550,000".into());
            ctx.insert("payment_deadline".into(), "2026-08-31".into());
            ctx.insert("client_name".into(), "テストクライアント株式会社".into());
            ctx.insert("contact_person".into(), "担当太郎".into());
            ctx.insert("display_name".into(), "テスト太郎".into());
            ctx.insert("month_display".into(), "2026年07月".into());
            ctx.insert("engineer_name".into(), "テスト太郎".into());
            ctx.insert("ym_str".into(), "2026年07月".into());
            ctx.insert("target_month_str".into(), "2026年07月".into());
            ctx.insert("year_month".into(), "2026年07月".into());
            ctx.insert("deadline".into(), "2026年07月05日".into());
            ctx.insert("days_pending".into(), "5".into());
            ctx.insert("address".into(), "東京都千代田区テスト1-1-1".into());
            ctx.insert("representative_name".into(), "代表太郎".into());
            ctx.insert("registration_no".into(), "T1234567890123".into());
            ctx.insert("progress_url".into(), format!("{}/partner-contracts", base_url));
            ctx.insert("contract_url".into(), format!("{}/partner-contracts/{}", base_url, partner_contract_id));
            ctx.insert("signed_at".into(), "2026-07-05 12:00:00".into());
            ctx.insert("signed_by".into(), "テスト承認者".into());
            ctx.insert("invite_url".into(), format!("{}/token/{}", base_url, order_uuid));
            ctx.insert("email".into(), to.into());
            ctx.insert("password".into(), "test-password-123".into());
            ctx.insert("work_report_url".into(), format!("{}/timesheets", base_url));
            ctx.insert("customer_name".into(), "テスト顧客株式会社".into());
            ctx.insert("username".into(), "管理者".into());
            ctx.insert("report_lines".into(), "  ・テスト太郎: 160.0h / 20日".into());
            ctx.insert("client_shared_url".into(), format!("{}/shared/test", base_url));

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
