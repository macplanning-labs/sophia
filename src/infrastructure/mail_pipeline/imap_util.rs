/// infrastructure/mail_pipeline/imap_util.rs — IMAP接続・MIME解析
///
/// IMAP の読み取りと MIME の解読は、社内共通の部品 `mail-ingest`（社内共通リポジトリの
/// backend/rust/mail-ingest。Cargo.toml を参照）にある（DEMO-000148）。ここでは、それをそのまま使えるように公開し、
/// Sophia 固有の「接続情報を自社情報（DB）から読む」処理だけを持つ。

pub use mail_ingest::*;

/// IMAP接続情報を読み込む。ユーザー/パスワードはs_company_info（自社情報画面のSMTP設定）と
/// 共通のGmailアカウント認証情報をそのまま使う。以前はIMAP_USER/IMAP_PASSWORD環境変数を
/// 別途持っていたが、DB側のSMTPパスワード更新にこの環境変数が追従せず認証失敗＝IMAP自動
/// ロックが再発する事故があったため、単一のソース（DB）に統一した（2026-08-18）。
/// ホスト/ポートはGmail固定でほぼ変わらず機密情報でもないため、引き続き環境変数（未設定時は
/// imap.gmail.com:993）を使う。
/// 受信サーバ(IMAP)のホストとポート。サーバの設定(環境変数 IMAP_HOST / IMAP_PORT)で決まり、未設定なら Gmail
pub fn imap_server() -> (String, u16) {
    let host = std::env::var("IMAP_HOST").unwrap_or_else(|_| "imap.gmail.com".to_string());
    let port: u16 = std::env::var("IMAP_PORT")
        .unwrap_or_else(|_| "993".to_string())
        .parse()
        .unwrap_or(993);
    (host, port)
}

pub async fn load_imap_config(pool: &sqlx::PgPool) -> Result<ImapConfig, String> {
    let (host, port) = imap_server();

    let creds = crate::infrastructure::repositories::company_info_repo::get_smtp_credentials(pool)
        .await
        .map_err(|e| {
            // anyhow::Error の内部の sqlx::Error を抽出
            if let Some(sqlx_err) = e.downcast_ref::<sqlx::Error>() {
                format!("自社情報のSMTP認証情報取得に失敗しました: {}", crate::infrastructure::mail_pipeline::ops_message::format_sqlx(sqlx_err))
            } else {
                format!("自社情報のSMTP認証情報取得に失敗しました: {e}")
            }
        })?;

    let (user, password) = creds.ok_or_else(|| {
        "IMAP認証情報が設定されていません（メール > 送信 の ⚙「メールアカウント」で、SMTPユーザー/パスワードを設定してください）".to_string()
    })?;

    Ok(ImapConfig { host, port, user, password })
}
