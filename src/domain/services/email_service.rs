/// domain/services/email_service.rs — メール送信サービス
///
/// lettre + DBテンプレート（s_email_template）でメール送信。
/// CompanyInfo から SMTP 設定取得、未設定時は .env フォールバック。
/// 全送信を h_sent_email に記録。
/// 12種の compose 関数を提供。
///
/// 添付ファイル付き送信: `send_with_attachment()` で PDF等を添付可能。

use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{info, warn};
use crate::infrastructure::repositories::email_template_repo;

/// メール送信エラー
#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("メールテンプレートが見つかりません: {0}")]
    TemplateNotFound(String),

    #[error("メールアドレス解析エラー: {0}")]
    AddressParse(String),

    #[error("メール構築エラー: {0}")]
    MessageBuild(String),

    #[error("SMTP接続エラー: {0}")]
    SmtpConnect(String),

    /// 実際のSMTP送信（DNS/認証/相手先拒否等）が失敗した場合。
    /// 呼び出し側はこれを見て「送付済み」ステータス更新等を行わないようにする。
    #[error("メール送信に失敗しました: {0}")]
    SendFailed(String),

    #[error("DBエラー: {0}")]
    Database(#[from] sqlx::Error),
}

type Result<T> = std::result::Result<T, EmailError>;

/// メール添付ファイル
pub struct EmailAttachment {
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

/// メール送信サービス
pub struct EmailService {
    pool: PgPool,
}

impl EmailService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// テンプレートコードからメール送信
    pub async fn send_by_template(
        &self,
        template_code: &str,
        to: &str,
        cc: Option<&str>,
        context: &HashMap<String, String>,
    ) -> Result<()> {
        let (subject, body) = self.render_template(template_code, context).await?;
        self.send(to, cc, &subject, &body).await
    }

    /// テンプレートコード+コンテキストから件名・本文を組み立てる（送信はしない）。
    /// メール送信前のプレビュー表示にも使う。
    pub async fn render_template(
        &self,
        template_code: &str,
        context: &HashMap<String, String>,
    ) -> Result<(String, String)> {
        // テンプレート取得
        let template = email_template_repo::find_template_by_code(&self.pool, template_code)
            .await
            .map_err(|e| EmailError::TemplateNotFound(format!("テンプレート取得失敗: {}", e)))?;

        let template = match template {
            Some(t) => t,
            None => {
                return Err(EmailError::TemplateNotFound(template_code.to_string()));
            }
        };

        // テンプレート変数置換（Tera簡易互換: {{ key }} → value）
        // company_name, company_tel がcontextに未設定の場合、s_company_infoから自動補完
        let mut full_context = context.clone();
        if !full_context.contains_key("company_name") || !full_context.contains_key("company_tel") {
            if let Ok(Some((company_name, company_tel))) = email_template_repo::get_company_info_name_tel(&self.pool).await {
                full_context.entry("company_name".into()).or_insert(company_name);
                full_context.entry("company_tel".into()).or_insert(company_tel);
            }
        }

        let mut subject = template.subject.clone();
        let mut body = template.body.clone();
        for (key, value) in &full_context {
            let placeholder = format!("{{{{ {} }}}}", key);
            subject = subject.replace(&placeholder, value);
            body = body.replace(&placeholder, value);
        }

        Ok((subject, body))
    }

    /// 直接メール送信（テキストのみ）
    pub async fn send(
        &self,
        to: &str,
        cc: Option<&str>,
        subject: &str,
        body: &str,
    ) -> Result<()> {
        self.send_with_attachment(to, cc, subject, body, vec![]).await
    }

    /// 添付ファイル付きメール送信
    ///
    /// 添付ファイルが空の場合はプレーンテキストメールとして送信。
    /// PDF等のバイナリを EmailAttachment で渡す。
    pub async fn send_with_attachment(
        &self,
        to: &str,
        cc: Option<&str>,
        subject: &str,
        body: &str,
        attachments: Vec<EmailAttachment>,
    ) -> Result<()> {
        // SMTP設定取得（CompanyInfo優先、.envフォールバック）
        let smtp_host = self.get_smtp_setting("email_host", "EMAIL_HOST").await;
        let smtp_port = self.get_smtp_setting("email_port", "EMAIL_PORT").await;
        let smtp_user = self.get_smtp_setting("email_host_user", "EMAIL_HOST_USER").await;
        let smtp_pass = self.get_smtp_setting("email_host_password", "EMAIL_HOST_PASSWORD").await;
        let from_email = self.get_smtp_setting("default_from_email", "DEFAULT_FROM_EMAIL").await;

        if smtp_host.is_empty() {
            info!("[Email] SMTP未設定のためスキップ: to={}, subject={}", to, subject);
            return Ok(());
        }

        // ステージング環境ではメール件名にプレフィックス付与
        let subject = match std::env::var("ENV_NAME").unwrap_or_default().as_str() {
            "staging" => format!("[STG] {}", subject),
            "local" => format!("[DEV] {}", subject),
            _ => subject.to_string(),
        };

        // 誤送信防止ガード（2026-07-12追加。請求書テストメールが実在取引先へ誤送信された
        // 事故を受けて導入。本番(ENV_NAME=production)以外では、社内ドメイン
        // (example.com) 以外の外部宛先への送信を実際には行わず、テスト用アドレス
        // （EMAIL_TEST_REDIRECT_TO、未設定時は差出人自身）へ強制的にリダイレクトする。
        // ENV_NAME が未設定・不明な値の場合も「本番ではない」扱いとし、fail-safeにする。
        let is_production = std::env::var("ENV_NAME").unwrap_or_default() == "production";
        let (to, cc, subject, body) = if is_production {
            (to.to_string(), cc.map(|s| s.to_string()), subject, body.to_string())
        } else if is_safe_test_recipient(to) {
            let safe_cc = cc.map(|c| {
                c.split(',')
                    .map(str::trim)
                    .filter(|a| !a.is_empty() && is_safe_test_recipient(a))
                    .collect::<Vec<_>>()
                    .join(",")
            });
            (to.to_string(), safe_cc.filter(|s| !s.is_empty()), subject, body.to_string())
        } else {
            let redirect_to = std::env::var("EMAIL_TEST_REDIRECT_TO")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| from_email.clone());
            if redirect_to.trim().is_empty() {
                return Err(EmailError::AddressParse(
                    "非本番の誤送信防止リダイレクト先が未設定です。EMAIL_TEST_REDIRECT_TO または default_from_email を設定してください".into(),
                ));
            }
            warn!(
                "[Email][誤送信防止ガード] 非本番環境(ENV_NAME={:?})から外部宛先への送信を検知。{} 宛の送信を {} へリダイレクトします",
                std::env::var("ENV_NAME").unwrap_or_default(), to, redirect_to
            );
            let redirected_subject = format!("[誤送信防止/本来の宛先: {}] {}", to, subject);
            let redirected_body = format!(
                "⚠️ 誤送信防止ガードにより、本来の宛先「{}」ではなくこのテストアドレスへリダイレクトされています。\n\
                本番環境でのみ実際の宛先へ送信されます。\n\n---\n\n{}",
                to, body
            );
            (redirect_to, None, redirected_subject, redirected_body)
        };
        let to = to.as_str();
        let cc = cc.as_deref();
        let body = body.as_str();

        use lettre::{Message, SmtpTransport, Transport};
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::message::{header::ContentType, MultiPart, SinglePart, Attachment};

        let from_addr = from_email.parse()
            .map_err(|e| EmailError::AddressParse(format!("From解析エラー: {}", e)))?;
        let to_addr = to.parse()
            .map_err(|e| EmailError::AddressParse(format!("To解析エラー: {}", e)))?;

        let mut email_builder = Message::builder()
            .from(from_addr)
            .to(to_addr)
            .subject(subject.clone());

        if let Some(cc_addr) = cc {
            if !cc_addr.is_empty() {
                for addr in cc_addr.split(',') {
                    let addr = addr.trim();
                    if !addr.is_empty() {
                        if let Ok(parsed) = addr.parse() {
                            email_builder = email_builder.cc(parsed);
                        }
                    }
                }
            }
        }

        let email = if attachments.is_empty() {
            // テキストのみ
            email_builder
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string())
                .map_err(|e| EmailError::MessageBuild(format!("{}", e)))?
        } else {
            // 添付ファイル付き（MultiPart）
            let text_part = SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string());

            let mut multipart = MultiPart::mixed()
                .singlepart(text_part);

            for att in &attachments {
                let ct: ContentType = att.content_type.parse()
                    .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
                let attachment = Attachment::new(att.filename.clone())
                    .body(att.content.clone(), ct);
                multipart = multipart.singlepart(attachment);
            }

            email_builder
                .multipart(multipart)
                .map_err(|e| EmailError::MessageBuild(format!("{}", e)))?
        };

        let port: u16 = smtp_port.parse().unwrap_or(587);
        let creds = Credentials::new(smtp_user.clone(), smtp_pass.clone());

        let mailer = SmtpTransport::starttls_relay(&smtp_host)
            .map_err(|e| EmailError::SmtpConnect(format!("{}", e)))?
            .port(port)
            .credentials(creds)
            .build();

        let send_result = mailer.send(&email);
        if let Err(e) = &send_result {
            use crate::infrastructure::mail_pipeline::ops_message;
            ops_message::log_error(ops_message::fail(
                "メール送信",
                "SMTP",
                "メール送信",
                "宛先へのメール未達（呼び出し元の業務処理は呼び出し側次第）",
                format!("to={} | {}", to, e),
            ));
        } else {
            info!("[Email] 送信成功: to={}, subject={}", to, subject);
        }

        // 送信ログをDB記録（送信の成否に関わらず「送信を試みた」記録として残す）
        if let Err(e) = email_template_repo::insert_sent_email_log(&self.pool, to, &subject, body).await {
            tracing::warn!("送信ログ記録失敗: {}", e);
        }

        // DB記録後に実送信の成否を呼び出し側へ伝播する。ここで握りつぶすと、
        // 呼び出し側が「送付済み」ステータス更新等を実送信の失敗時にも行ってしまう
        send_result.map_err(|e| EmailError::SendFailed(format!("{}", e)))?;

        Ok(())
    }

    async fn get_smtp_setting(&self, db_field: &str, env_key: &str) -> String {
        // CompanyInfo（DB）優先、空なら .env フォールバック
        let db_val = email_template_repo::get_smtp_column(&self.pool, db_field)
            .await
            .ok()
            .flatten();

        match db_val {
            Some(v) if !v.trim().is_empty() => v,
            _ => std::env::var(env_key).unwrap_or_default(),
        }
    }
}

/// 非本番環境で実送信してよい宛先かどうかを判定する（誤送信防止ガード）。
///
/// 社内ドメイン（example.com）宛、または `EMAIL_TEST_ALLOWED_RECIPIENTS`
/// （カンマ区切り）に明示的に列挙されたアドレス宛のみ許可する。
fn is_safe_test_recipient(addr: &str) -> bool {
    is_recipient_allowed(addr, &std::env::var("EMAIL_TEST_ALLOWED_RECIPIENTS").unwrap_or_default())
}

/// `is_safe_test_recipient` の判定ロジック本体（env非依存・テスト容易化のため分離）
fn is_recipient_allowed(addr: &str, allowlist_csv: &str) -> bool {
    let addr_lower = addr.trim().to_lowercase();
    if addr_lower.ends_with("@example.com") {
        return true;
    }
    allowlist_csv
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .any(|allowed| !allowed.is_empty() && allowed == addr_lower)
}

/// 社内通知先メールアドレスを取得する（EDI互換: get_notify_email）
///
/// CompanyInfo の default_from_email を返す。
/// 未設定時は .env の DEFAULT_FROM_EMAIL にフォールバック。
pub async fn get_notify_email(pool: &PgPool) -> String {
    let db_val = email_template_repo::get_default_from_email(pool)
        .await
        .ok()
        .flatten();

    match db_val {
        Some(v) if !v.trim().is_empty() => v,
        _ => std::env::var("DEFAULT_FROM_EMAIL").unwrap_or_default(),
    }
}

// ============================================================
// compose 関数群（12種）
// ============================================================

/// "2026年07月" のような表記から件名用の短い月表記 "07月" を取り出す。
/// "年" が見つからない場合はそのまま返す（想定外フォーマットへのフォールバック）。
fn month_short(month: &str) -> String {
    match month.split_once('年') {
        Some((_, rest)) => rest.to_string(),
        None => month.to_string(),
    }
}

/// 送信日 + 有効期限日数から、件名用の締切日表記 "MM月DD日" を組み立てる。
fn deadline_date_from_expiry(expiry_days: i32) -> String {
    let deadline = chrono::Utc::now().date_naive() + chrono::Duration::days(expiry_days as i64);
    deadline.format("%m月%d日").to_string()
}

/// 注文書送付メールを組み立てる（`order_send`テンプレート用）
pub fn compose_order_publish_email(
    partner_name: &str,
    order_id: &str,
    month: &str,
    token_url: &str,
    expiry_days: i32,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("month_short".into(), month_short(month));
    ctx.insert("token_url".into(), token_url.into());
    ctx.insert("expiry_days".into(), expiry_days.to_string());
    ctx.insert("deadline_date".into(), deadline_date_from_expiry(expiry_days));
    ctx
}

/// 注文書承諾通知（社内向け）を組み立てる
/// EDI互換: core/domain/services/email_service.py compose_order_approve_email
pub fn compose_order_approve_email(
    partner_name: &str,
    order_id: &str,
    project_name: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("project_name".into(), project_name.into());
    ctx
}

/// 注文書承認リマインドを組み立てる
/// EDI互換: core/domain/services/email_service.py compose_order_approve_reminder_email
/// （EDI_MP版はproject_name/work_start/work_endも使用するが、Rust版はdays_pendingで代替し
/// 経過日数を明示する形に強化。token_urlはEDI_MP版のlogin_urlに相当）
pub fn compose_order_approve_reminder_email(
    partner_name: &str,
    order_id: &str,
    days_pending: i32,
    token_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("days_pending".into(), days_pending.to_string());
    ctx.insert("token_url".into(), token_url.into());
    ctx
}

/// 支払通知書送付メールを組み立てる（`notice_send`テンプレート用）
pub fn compose_payment_notice_email(
    partner_name: &str,
    notice_id: &str,
    month: &str,
    token_url: &str,
    expiry_days: i32,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("notice_id".into(), notice_id.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("month_short".into(), month_short(month));
    ctx.insert("token_url".into(), token_url.into());
    ctx.insert("expiry_days".into(), expiry_days.to_string());
    ctx.insert("deadline_date".into(), deadline_date_from_expiry(expiry_days));
    ctx
}

/// 請求書送付メールを組み立てる（`client_invoice_send` / `client_invoice_send_first`テンプレート用）
///
/// 支払通知書（compose_payment_notice_email）と同様、PDF添付ではなくトークンURL経由の
/// 確認・ダウンロード方式のため、token_url/expiry_daysも渡す必要がある。
pub fn compose_invoice_email(
    client_name: &str,
    invoice_id: &str,
    month: &str,
    total: i64,
    due_date: &str,
    token_url: &str,
    expiry_days: i32,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("client_name".into(), client_name.into());
    ctx.insert("invoice_id".into(), invoice_id.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("total".into(), format_yen(total));
    ctx.insert("due_date".into(), due_date.into());
    ctx.insert("token_url".into(), token_url.into());
    ctx.insert("expiry_days".into(), expiry_days.to_string());
    ctx.insert("deadline_date".into(), deadline_date_from_expiry(expiry_days));
    ctx
}

/// 3桁区切りの円表記に整形する（例: 1234567 → "1,234,567"）
fn format_yen(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if n < 0 { format!("-{}", formatted) } else { formatted }
}

/// 支払通知書承諾通知（社内向け）を組み立てる
/// EDI互換: core/domain/services/email_service.py compose_invoice_approve_email
/// （EDI_MP版はinvoice_no/invoice_url等パートナー請求書向けの文脈だが、Sophiaでは
/// 「自社→パートナー」の支払通知書(t_payment_notice)承諾通知として使用）
pub fn compose_invoice_approve_email(
    partner_name: &str,
    invoice_id: &str,
    target_month: &str,
    total: i64,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("invoice_id".into(), invoice_id.into());
    ctx.insert("target_month".into(), target_month.into());
    ctx.insert("total_amount".into(), format_yen(total));
    ctx
}

/// クライアント請求書 受領確認通知（社内向け）を組み立てる
pub fn compose_invoice_receipt_confirmed_email(
    client_name: &str,
    invoice_id: &str,
    target_month: &str,
    total: i64,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("client_name".into(), client_name.into());
    ctx.insert("invoice_id".into(), invoice_id.into());
    ctx.insert("target_month".into(), target_month.into());
    ctx.insert("total_amount".into(), format_yen(total));
    ctx
}

/// 支払通知書承諾リマインドを組み立てる（`invoice_approve_reminder`テンプレート用）
/// EDI互換: core/domain/services/email_service.py compose_invoice_approve_reminder_email
pub fn compose_invoice_approve_reminder_email(
    partner_name: &str,
    invoice_id: &str,
    target_month: &str,
    total: i64,
    token_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("invoice_id".into(), invoice_id.into());
    ctx.insert("target_month".into(), target_month.into());
    ctx.insert("total_amount".into(), format_yen(total));
    ctx.insert("token_url".into(), token_url.into());
    ctx
}

/// 稼働報告書クライアント送付メールを組み立てる
pub fn compose_work_report_share_email(
    client_name: &str,
    month: &str,
    engineer_name: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("client_name".into(), client_name.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("engineer_name".into(), engineer_name.into());
    ctx
}

/// 提出期限の曜日を日本語1文字で返す
fn weekday_ja(date: chrono::NaiveDate) -> &'static str {
    use chrono::{Datelike, Weekday};
    match date.weekday() {
        Weekday::Mon => "月",
        Weekday::Tue => "火",
        Weekday::Wed => "水",
        Weekday::Thu => "木",
        Weekday::Fri => "金",
        Weekday::Sat => "土",
        Weekday::Sun => "日",
    }
}

/// 稼働報告催促メール（`work_report_reminder`テンプレート用）に必要な、
/// 提出期限との位置関係に応じた文面（冒頭文・件名の緊急度タグ・対応依頼の言い回し）を組み立てる。
///
/// - 期限超過: 「まだご登録が確認できておりません」＋「至急」
/// - 期限当日: 「本日が登録期限」＋「本日中に」
/// - 期限前:   「◯月◯日（◯）までのご登録をお願いいたします」＋「期限までに」
fn work_report_reminder_wording(
    month_short: &str,
    deadline: chrono::NaiveDate,
) -> (String, String, String, String) {
    let today = chrono::Utc::now().date_naive();
    let deadline_display = format!("{}（{}）", deadline.format("%m月%d日"), weekday_ja(deadline));

    let (deadline_message, action_phrase, subject_urgency) = if today > deadline {
        (
            format!("{} 分の勤務表について、{} が提出期限でしたが、まだご登録が確認できておりません。", month_short, deadline_display),
            "至急",
            "【至急】",
        )
    } else if today == deadline {
        (
            format!("{} 分の勤務表について、本日が登録期限となっております。", month_short),
            "本日中に",
            "【本日締切】",
        )
    } else {
        (
            format!("{} 分の勤務表について、{} までのご登録をお願いいたします。", month_short, deadline_display),
            "期限までに",
            "【ご登録のお願い】",
        )
    };

    (deadline_message, deadline_display, action_phrase.to_string(), subject_urgency.to_string())
}

/// 稼働報告催促メールを組み立てる（`work_report_reminder`テンプレート用）
///
/// 自動リマインドジョブ専用。実際の提出期限（`t_monthly_task.deadline`）との
/// 位置関係（期限前／当日／超過後）で文面を出し分ける。
/// `engineer_names` は「山田太郎さん、鈴木一郎さん」のように、対象パートナー配下で
/// 未登録のエンジニア全員をさん付けで連結した文字列を渡す（複数名は1通にまとめる）。
pub fn compose_work_report_reminder_email(
    engineer_names: &str,
    month_short: &str,
    token_url: &str,
    deadline: chrono::NaiveDate,
) -> HashMap<String, String> {
    let (deadline_message, deadline_display, action_phrase, subject_urgency) =
        work_report_reminder_wording(month_short, deadline);

    let mut ctx = HashMap::new();
    ctx.insert("deadline_message".into(), deadline_message);
    ctx.insert("engineer_names".into(), engineer_names.into());
    ctx.insert("month_short".into(), month_short.into());
    ctx.insert("deadline_display".into(), deadline_display);
    ctx.insert("token_url".into(), token_url.into());
    ctx.insert("action_phrase".into(), action_phrase);
    ctx.insert("subject_urgency".into(), subject_urgency);
    ctx
}

/// 稼働報告 初回依頼メールを組み立てる（`work_report_request`テンプレート用）
///
/// 発注書詳細の「稼働報告の提出依頼メール」ボタン専用。まだ未提出であることを
/// 前提としない中立的な文面のため、work_report_reminder とはテンプレートを分けている。
/// `deadline` は実際の提出期限（既存タスクがあればその期限、無ければ呼び出し側でのフォールバック値）。
pub fn compose_work_report_request_email(
    partner_name: &str,
    month: &str,
    token_url: &str,
    deadline: chrono::NaiveDate,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("token_url".into(), token_url.into());
    ctx.insert("deadline_date".into(), deadline.format("%m月%d日").to_string());
    ctx
}

/// 稼働報告 初回依頼の送り忘れ防止・社内通知メールを組み立てる（`work_report_request_internal_notify`テンプレート用）
///
/// パートナーへは送らず、`EmailService::get_notify_email` の宛先（社内）へ送る。
/// クロス等から受け取ったPDFを添付して手動で送る運用のため、自動送信はせず
/// 「そろそろ送るタイミングです」という気付き用の通知に留める。
pub fn compose_work_report_request_internal_notify_email(
    partner_name: &str,
    project_name: &str,
    order_id: &str,
    month: &str,
    order_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("project_name".into(), project_name.into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("month".into(), month.into());
    ctx.insert("order_url".into(), order_url.into());
    ctx
}

/// 稼働報告期限リマインドを組み立てる
pub fn compose_work_report_deadline_reminder_email(
    partner_name: &str,
    year_month: &str,
    deadline: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("year_month".into(), year_month.into());
    ctx.insert("deadline".into(), deadline.into());
    ctx
}

/// パートナー招待メールを組み立てる
pub fn compose_invitation_email(
    partner_name: &str,
    invite_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("engineer_name".into(), partner_name.into());
    ctx.insert("invite_url".into(), invite_url.into());
    ctx
}

/// 基本契約書送付メールを組み立てる
pub fn compose_contract_send_email(
    partner_name: &str,
    token_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("token_url".into(), token_url.into());
    ctx
}

/// パートナー基本情報登録完了通知メールを組み立てる（自社担当者宛）
/// EDI互換: core/utils.py compose_partner_info_registered_email
pub fn compose_partner_info_registered_email(
    partner_name: &str,
    address: &str,
    representative_name: &str,
    registration_no: &str,
    progress_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("address".into(), address.into());
    ctx.insert("representative_name".into(), representative_name.into());
    ctx.insert("registration_no".into(), registration_no.into());
    ctx.insert("progress_url".into(), progress_url.into());
    ctx
}

/// 基本契約書承諾通知メールを組み立てる（自社担当者宛）
/// EDI互換: core/utils.py compose_contract_approve_email
pub fn compose_contract_approve_email(
    partner_name: &str,
    contract_url: &str,
    signed_at: &str,
    signed_by: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("contract_url".into(), contract_url.into());
    ctx.insert("signed_at".into(), signed_at.into());
    ctx.insert("signed_by".into(), signed_by.into());
    ctx
}

/// 稼働報告確定通知メールを組み立てる（担当者宛）
/// EDI互換: invoices/services/invoice_service.py _send_work_report_notification
pub fn compose_report_approved_email(
    display_name: &str,
    month_display: &str,
    username: &str,
    report_lines: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("display_name".into(), display_name.into());
    ctx.insert("month_display".into(), month_display.into());
    ctx.insert("username".into(), username.into());
    ctx.insert("report_lines".into(), report_lines.into());
    ctx
}

/// 注文書訂正再送付メールを組み立てる（パートナー宛）
/// EDI互換: orders/services/order_service.py republish_order
pub fn compose_order_republish_email(
    partner_name: &str,
    order_id: &str,
    order_url: &str,
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("partner_name".into(), partner_name.into());
    ctx.insert("order_id".into(), order_id.into());
    ctx.insert("order_url".into(), order_url.into());
    ctx
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::{is_recipient_allowed, month_short};

    #[test]
    fn internal_domain_is_always_allowed() {
        assert!(is_recipient_allowed("y.yoshikawa@example.com", ""));
        assert!(is_recipient_allowed("Y.Yoshikawa@MacPlanning.COM", ""));
    }

    #[test]
    fn external_domain_is_blocked_by_default() {
        assert!(!is_recipient_allowed("client@ntp.example.com", ""));
    }

    #[test]
    fn external_domain_allowed_when_explicitly_listed() {
        let allowlist = "client@ntp.example.com, partner@example.co.jp";
        assert!(is_recipient_allowed("client@ntp.example.com", allowlist));
        assert!(is_recipient_allowed(" partner@example.co.jp ", allowlist));
        assert!(!is_recipient_allowed("other@example.com", allowlist));
    }

    #[test]
    fn empty_allowlist_entries_are_ignored() {
        assert!(!is_recipient_allowed("", ",,  ,"));
    }

    #[test]
    fn month_short_strips_year_prefix() {
        assert_eq!(month_short("2026年07月"), "07月");
        assert_eq!(month_short("07月"), "07月"); // 年なし表記への耐性
    }

    #[test]
    fn work_report_reminder_wording_before_deadline() {
        let today = chrono::Utc::now().date_naive();
        let deadline = today + chrono::Duration::days(5);
        let (message, action, urgency) = work_report_reminder_wording("07月", deadline);
        assert!(message.contains("までのご登録をお願いいたします"), "{message}");
        assert_eq!(action, "期限までに");
        assert_eq!(urgency, "【ご登録のお願い】");
    }

    #[test]
    fn work_report_reminder_wording_due_today() {
        let today = chrono::Utc::now().date_naive();
        let (message, action, urgency) = work_report_reminder_wording("07月", today);
        assert!(message.contains("本日が登録期限となっております"), "{message}");
        assert_eq!(action, "本日中に");
        assert_eq!(urgency, "【本日締切】");
    }

    #[test]
    fn work_report_reminder_wording_overdue() {
        let today = chrono::Utc::now().date_naive();
        let deadline = today - chrono::Duration::days(3);
        let (message, action, urgency) = work_report_reminder_wording("07月", deadline);
        assert!(message.contains("まだご登録が確認できておりません"), "{message}");
        assert_eq!(action, "至急");
        assert_eq!(urgency, "【至急】");
    }

    fn work_report_reminder_wording(month_short: &str, deadline: chrono::NaiveDate) -> (String, String, String) {
        let (message, _display, action, urgency) = super::work_report_reminder_wording(month_short, deadline);
        (message, action, urgency)
    }
}
