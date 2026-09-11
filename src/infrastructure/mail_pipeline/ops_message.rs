/// infrastructure/mail_pipeline/ops_message.rs — 運用監視向けエラー文言
///
/// Google Chat のログキーワード監視にそのまま流れるため、
/// 「どの処理が」「何に失敗し」「業務影響は何か」が一文で分かる形式に統一する。
///
/// 形式:
/// `[メール取込/{phase}] 処理={process} 結果=失敗 影響={impact} | {detail}`

/// Phase1致命障害の原因種別分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase1FatalKind {
    Database,      // DB 通信・プール
    ImapAuth,      // AUTHENTICATIONFAILED / ロック
    ImapNetwork,   // 接続・TLS・タイムアウト（認証以外）
    ConfigMissing, // 自社情報に SMTP ユーザー/パスワードが無い
    Other,
}

/// エラー文字列から Phase1FatalKind を判定する（大小文字無視）
pub fn classify_phase1_fatal(error: &str) -> Phase1FatalKind {
    let error_lower = error.to_lowercase();

    // 1. IMAP認証関連
    if error_lower.contains("authenticationfailed")
        || error_lower.contains("imap読取(認証)")
        || error_lower.contains("ロック中")
    {
        return Phase1FatalKind::ImapAuth;
    }

    // 2. データベース関連（before ConfigMissing check）
    if error_lower.contains("error communicating with database")
        || error_lower.contains("connection reset")
        || error_lower.contains("connection refused")
            && (error_lower.contains("自社情報")
                || error_lower.contains("database")
                || error_lower.contains("dbエラー")
                || error_lower.contains("sqlx")
                || error_lower.contains("os error 54")
                || error_lower.contains("os error 104")
                || error_lower.contains("server closed the connection")
                || error_lower.contains("broken pipe"))
        || error_lower.contains("pool timed out")
        || error_lower.contains("自社情報のsmtp認証情報取得に失敗")
    {
        return Phase1FatalKind::Database;
    }

    // 3. 自社情報に設定がない
    if error_lower.contains("imap認証情報が設定されていません") {
        return Phase1FatalKind::ConfigMissing;
    }

    // 4. IMAP接続・ネットワーク関連
    if (error_lower.contains("imap接続")
        || error_lower.contains("imapホスト")
        || error_lower.contains("imapログイン")
        || error_lower.contains("tls")
        || error_lower.contains("timeout"))
        && error_lower.contains("imap")
    {
        return Phase1FatalKind::ImapNetwork;
    }

    // 5. その他
    Phase1FatalKind::Other
}

/// Chat・メール文面用のコピー（件名核とアドバイス）
pub struct FatalAlertCopy {
    pub title: String,   // Chat太字・メール件名の核（日時は呼び出し側で付与）
    pub advice: String,  // 末尾の確認手順（1〜3文）
}

/// Phase1FatalKind から Chat・メール用のコピーを生成
pub fn fatal_alert_copy(kind: Phase1FatalKind) -> FatalAlertCopy {
    match kind {
        Phase1FatalKind::Database => FatalAlertCopy {
            title: "メール自動取込が停止（データベース接続）".to_string(),
            advice: "原因は IMAP ではなく、認証情報を読もうとした PostgreSQL との通信失敗です。MINISFORUM 上の sophia-prod-db の生存・接続数・再起動直後の切断を確認してください。IMAP のパスワード変更は不要です。".to_string(),
        },
        Phase1FatalKind::ImapAuth => FatalAlertCopy {
            title: "メール自動取込が停止（IMAP認証）".to_string(),
            advice: "Gmail アプリパスワード／自社情報の SMTP ユーザーが正しいか確認し、ロック中なら管理者画面で IMAP ロック解除してください。".to_string(),
        },
        Phase1FatalKind::ImapNetwork => FatalAlertCopy {
            title: "メール自動取込が停止（IMAP接続）".to_string(),
            advice: "imap.gmail.com:993 への疎通・TLS・タイムアウトを確認してください。".to_string(),
        },
        Phase1FatalKind::ConfigMissing => FatalAlertCopy {
            title: "メール自動取込が停止（認証情報未設定）".to_string(),
            advice: "自社情報画面の SMTP ユーザー／パスワードを設定してください。".to_string(),
        },
        Phase1FatalKind::Other => FatalAlertCopy {
            title: "メール自動取込が停止（メール読取段階）".to_string(),
            advice: "下記エラー内容を見て原因を切り分けてください。IMAP と決めつけないでください。".to_string(),
        },
    }
}

/// 運用向け失敗メッセージを組み立てる
pub fn fail(area: &str, location: &str, process: &str, impact: &str, detail: impl std::fmt::Display) -> String {
    format!("[{area}/{location}] 処理={process} 結果=失敗 影響={impact} | {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fail_format() {
        let msg = fail("メール取込", "Phase1", "IMAP読取", "今回の新着メール走査不可", "test error");
        assert!(msg.contains("[メール取込/Phase1]"));
        assert!(msg.contains("処理=IMAP読取"));
        assert!(msg.contains("結果=失敗"));
        assert!(msg.contains("影響=今回の新着メール走査不可"));
        assert!(msg.contains("test error"));
    }

    #[test]
    fn test_classify_phase1_fatal_database() {
        let error = "自社情報のSMTP認証情報取得に失敗しました: error communicating with database: Connection reset by peer (os error 54)";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::Database);
    }

    #[test]
    fn test_classify_phase1_fatal_imap_auth() {
        let error = "NO [AUTHENTICATIONFAILED] Invalid credentials";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapAuth);
    }

    #[test]
    fn test_classify_phase1_fatal_imap_network() {
        let error = "IMAP接続タイムアウト";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapNetwork);
    }

    #[test]
    fn test_classify_phase1_fatal_config_missing() {
        let error = "IMAP認証情報が設定されていません";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ConfigMissing);
    }

    #[test]
    fn test_fatal_alert_copy_database() {
        let copy = fatal_alert_copy(Phase1FatalKind::Database);
        assert!(copy.title.contains("データベース"));
        assert!(copy.advice.contains("IMAP ではなく"));
    }

    #[test]
    fn test_fatal_alert_copy_imap_auth() {
        let copy = fatal_alert_copy(Phase1FatalKind::ImapAuth);
        assert!(copy.title.contains("IMAP認証"));
    }
}

/// sqlx エラーを短く整形（PgDatabaseError の Debug 全文は出さない）
pub fn format_sqlx(e: &sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.to_string()).unwrap_or_else(|| "-".into());
            let msg = db.message();
            format!("DBエラー code={code}: {msg}")
        }
        other => format!("DBエラー: {other}"),
    }
}

/// ERROR レベルで出力（監視キーワード `ERROR` に載る）
pub fn log_error(message: impl AsRef<str>) {
    tracing::error!("{}", message.as_ref());
}

/// フェーズ結果に溜めたエラーを監視向けに一括出力
pub fn log_errors(errors: &[String]) {
    for e in errors {
        log_error(e);
    }
}
