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
    ImapProtocol,  // IMAP サーバーからの予期しない応答（認証以外）
    ConfigMissing, // 自社情報に SMTP ユーザー/パスワードが無い
    Other,
}

/// IMAP エラーの処理段階
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapStage {
    Resolve,   // DNS 名前解決
    Connect,   // TCP 接続・TLS ハンドシェイク
    Login,     // ログイン
    Examine,   // INBOX オープン
    Search,    // メール検索
    Fetch,     // メール本文取得
    Unknown,   // 不明
}

impl ImapStage {
    pub fn name_japanese(&self) -> &'static str {
        match self {
            ImapStage::Resolve => "Gmailサーバーの住所確認",
            ImapStage::Connect => "Gmailへの接続",
            ImapStage::Login => "Gmailへのログイン",
            ImapStage::Examine => "受信箱を開く処理",
            ImapStage::Search => "新着メールの検索",
            ImapStage::Fetch => "メール本文の取得",
            ImapStage::Unknown => "メール読取",
        }
    }
}

/// エラー文字列から IMAP 処理段階を判定する
pub fn imap_stage(error: &str) -> ImapStage {
    let error_lower = error.to_lowercase();

    if error_lower.contains("imap名前解決") {
        ImapStage::Resolve
    } else if error_lower.contains("imap接続エラー(tcp)")
        || error_lower.contains("imap接続エラー(tls初期化)")
        || error_lower.contains("imap接続エラー(tls)")
    {
        ImapStage::Connect
    } else if error_lower.contains("imapログイン") {
        ImapStage::Login
    } else if error_lower.contains("imap受信箱オープンエラー") {
        ImapStage::Examine
    } else if error_lower.contains("imap検索エラー") {
        ImapStage::Search
    } else if error_lower.contains("imap取得エラー") {
        ImapStage::Fetch
    } else {
        ImapStage::Unknown
    }
}

/// エラー文字列から Phase1FatalKind を判定する（大小文字無視）
pub fn classify_phase1_fatal(error: &str) -> Phase1FatalKind {
    let error_lower = error.to_lowercase();

    // 1. IMAP認証関連（AUTHENTICATIONFAILED、ロック中など）
    if error_lower.contains("authenticationfailed")
        || error_lower.contains("imap読取(認証)")
        || error_lower.contains("ロック中")
    {
        return Phase1FatalKind::ImapAuth;
    }

    // 2. 自社情報に設定がない
    if error_lower.contains("imap認証情報が設定されていません") {
        return Phase1FatalKind::ConfigMissing;
    }

    // IMAP 通信由来のエラーは imap_util が必ず「IMAP〜エラー」で始める。
    // ops_message::fail で包んだ一行（「処理=IMAP読取」等を含む）ではなく、生のエラー文字列を渡すこと。
    let from_imap = error_lower.starts_with("imap");

    // 3. データベース関連。素の "connection reset" 等は IMAP の切断でも出るため、DB の根拠にしない
    if !from_imap
        && (error_lower.contains("自社情報のsmtp認証情報取得に失敗")
            || error_lower.contains("error communicating with database")
            || error_lower.contains("pool timed out")
            || error_lower.contains("チェックポイント更新失敗")
            || error_lower.contains("dbエラー"))
    {
        return Phase1FatalKind::Database;
    }

    // 4. IMAP 起因のエラー: 通信系（再試行で直りうる）か、サーバーの想定外の応答か
    if from_imap {
        // IMAP ネットワークエラーのキーワード（大小文字無視）
        let network_keywords = [
            "connection lost",
            "timed out",
            "timeout",
            "タイムアウト",
            "resource temporarily unavailable",
            "os error",
            "broken pipe",
            "connection reset",
            "connection refused",
            "connection aborted",
            "tls",
            "名前解決",
            "would block",
            "unexpected eof",
        ];

        let is_network_error = network_keywords.iter().any(|kw| error_lower.contains(kw));
        let stage = imap_stage(error);
        let is_network_stage = matches!(stage, ImapStage::Resolve | ImapStage::Connect);

        if is_network_error || is_network_stage {
            return Phase1FatalKind::ImapNetwork;
        } else {
            return Phase1FatalKind::ImapProtocol;
        }
    }

    // 5. その他
    Phase1FatalKind::Other
}

/// Phase1 の IMAP 一時障害としてリトライ対象か（認証失敗・設定欠落・DB障害は対象外）。
pub fn is_retryable_phase1_imap_error(error: &str) -> bool {
    matches!(classify_phase1_fatal(error), Phase1FatalKind::ImapNetwork)
}

/// Chat・メール文面用のコピー（リッチ構造）
pub struct FatalAlertCopy {
    pub title: String,          // Chat太字・メール件名の核（日時は呼び出し側で付与）
    pub summary: String,        // 何が起きたか（平易な一文、{stage} 変数可）
    pub likely_cause: String,   // 考えられる原因
    pub action: String,         // 必要な対応（「対応不要」なら明記）
    pub action_required: bool,  // true=【要対応】、false=【対応不要（自動で再試行中）】
}

/// Phase1FatalKind から Chat・メール用のコピーを生成
pub fn fatal_alert_copy(kind: Phase1FatalKind, stage: ImapStage) -> FatalAlertCopy {
    match kind {
        Phase1FatalKind::Database => FatalAlertCopy {
            title: "メール自動取込が停止（データベース接続）".to_string(),
            summary: "メール読取に必要な認証情報をデータベース(sophia-prod-db)から読もうとしましたが、接続に失敗しました。".to_string(),
            likely_cause: "データベースコンテナ(sophia-oss-db) の障害、または社内ネットワーク障害。".to_string(),
            action: "データベースコンテナ(sophia-oss-db) の生存・接続数・再起動直後の切断を確認してください。IMAP のパスワード変更は不要です。".to_string(),
            action_required: true,
        },
        Phase1FatalKind::ImapAuth => FatalAlertCopy {
            title: "メール自動取込が停止（IMAP認証）".to_string(),
            summary: "Gmail への認証に失敗しました。".to_string(),
            likely_cause: "Gmail アプリパスワード、または自社情報画面の SMTP ユーザー/パスワードが正しくない、または Sophia 管理者画面で IMAP がロック中。".to_string(),
            action: "① 自社情報画面の SMTP ユーザー／パスワードが正しいか確認。② ロック中なら管理者画面で IMAP ロック解除をしてください。".to_string(),
            action_required: true,
        },
        Phase1FatalKind::ImapNetwork => FatalAlertCopy {
            title: "メール自動取込が一時停止（Gmailとの通信障害）".to_string(),
            summary: format!("「{}」の途中で Gmail との通信が切れました。", stage.name_japanese()),
            likely_cause: "Gmail 側の一時的な障害、または社内ネットワーク／インターネット回線の瞬断。".to_string(),
            action: "対応不要です。自動で再試行し、復旧したら復旧のお知らせを送ります。数時間続く場合は サーバーのネットワークと Google Workspace のステータス（https://www.google.com/appsstatus）を確認してください。".to_string(),
            action_required: false,
        },
        Phase1FatalKind::ImapProtocol => FatalAlertCopy {
            title: "メール自動取込が停止（Gmail応答エラー）".to_string(),
            summary: "Gmail が想定外の応答を返しました。".to_string(),
            likely_cause: "Gmail 側の一時的な不具合、または Sophia と Gmail 間の通信内容の不整合。".to_string(),
            action: "下記エラー内容を確認してください。一度だけの発生なら、次回自動実行時に復旧している可能性があります。繰り返し発生する場合は開発者に詳細を連絡してください。".to_string(),
            action_required: true,
        },
        Phase1FatalKind::ConfigMissing => FatalAlertCopy {
            title: "メール自動取込が停止（認証情報未設定）".to_string(),
            summary: "Gmail 認証情報が設定されていません。".to_string(),
            likely_cause: "自社情報画面の SMTP ユーザー／パスワードが設定されていない。".to_string(),
            action: "自社情報画面の SMTP ユーザー／パスワードを設定してください。".to_string(),
            action_required: true,
        },
        Phase1FatalKind::Other => FatalAlertCopy {
            title: "メール自動取込が停止（メール読取段階）".to_string(),
            summary: "メール読取処理中に予期しないエラーが発生しました。".to_string(),
            likely_cause: "エラー内容から自動分類できませんでした。下記エラー詳細を参照してください。".to_string(),
            action: "下記エラー内容の詳細を確認し、IMAP 設定・Gmail 設定・ネットワーク・データベース接続のいずれが原因かを切り分けてください。それでもわからない場合は開発者に詳細を連絡してください。".to_string(),
            action_required: true,
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
    fn test_classify_phase1_fatal_imap_network_fetch() {
        // IMAP取得(FETCH)エラーで Connection Lost — ネットワークエラー
        let error = "IMAP取得エラー(FETCH): Connection Lost";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapNetwork);
    }

    #[test]
    fn test_classify_phase1_fatal_imap_network_tcp() {
        // TCP接続エラーは Resolve/Connect ステージ — ネットワークエラー
        let error = "IMAP接続エラー(TCP): Connection refused (os error 111)";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapNetwork);
    }

    #[test]
    fn test_classify_phase1_fatal_imap_network_search_reset() {
        // IMAP検索エラーで connection reset — ネットワークエラー（DB ではない）
        let error = "IMAP検索エラー(SEARCH): connection reset by peer";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapNetwork);
    }

    #[test]
    fn test_classify_phase1_fatal_imap_protocol() {
        // IMAP サーバーからの予期しない応答（非ネットワークエラー）
        let error = "IMAP取得エラー(FETCH): No Response: Some unexpected thing";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ImapProtocol);
    }

    #[test]
    fn test_is_retryable_phase1_imap_error() {
        assert!(is_retryable_phase1_imap_error(
            "IMAP取得エラー(FETCH): Connection Lost"
        ));
        assert!(is_retryable_phase1_imap_error(
            "IMAPログインエラー: Resource temporarily unavailable (os error 11)"
        ));
        assert!(is_retryable_phase1_imap_error(
            "IMAP接続エラー(TCP): Connection refused (os error 111)"
        ));
        assert!(!is_retryable_phase1_imap_error(
            "NO [AUTHENTICATIONFAILED] Invalid credentials"
        ));
        assert!(!is_retryable_phase1_imap_error(
            "IMAP認証情報が設定されていません"
        ));
        assert!(!is_retryable_phase1_imap_error(
            "自社情報のSMTP認証情報取得に失敗しました: error communicating with database"
        ));
        assert!(!is_retryable_phase1_imap_error(
            "IMAP取得エラー(FETCH): No Response: Some unexpected thing"
        ));
    }

    #[test]
    fn test_classify_phase1_fatal_config_missing() {
        let error = "IMAP認証情報が設定されていません";
        assert_eq!(classify_phase1_fatal(error), Phase1FatalKind::ConfigMissing);
    }

    #[test]
    fn test_imap_stage_resolve() {
        assert_eq!(imap_stage("IMAP名前解決エラー: host not found"), ImapStage::Resolve);
    }

    #[test]
    fn test_imap_stage_connect() {
        assert_eq!(imap_stage("IMAP接続エラー(TCP): Connection refused"), ImapStage::Connect);
        assert_eq!(imap_stage("IMAP接続エラー(TLS): handshake error"), ImapStage::Connect);
    }

    #[test]
    fn test_imap_stage_login() {
        assert_eq!(imap_stage("IMAPログインエラー: Invalid credentials"), ImapStage::Login);
    }

    #[test]
    fn test_imap_stage_fetch() {
        assert_eq!(imap_stage("IMAP取得エラー(FETCH): Connection Lost"), ImapStage::Fetch);
    }

    #[test]
    fn test_fatal_alert_copy_database() {
        let copy = fatal_alert_copy(Phase1FatalKind::Database, ImapStage::Unknown);
        assert!(copy.title.contains("データベース"));
        assert!(copy.action_required);
    }

    #[test]
    fn test_fatal_alert_copy_imap_auth() {
        let copy = fatal_alert_copy(Phase1FatalKind::ImapAuth, ImapStage::Login);
        assert!(copy.title.contains("IMAP認証"));
        assert!(copy.action_required);
    }

    #[test]
    fn test_fatal_alert_copy_imap_network() {
        let copy = fatal_alert_copy(Phase1FatalKind::ImapNetwork, ImapStage::Fetch);
        assert!(copy.title.contains("通信障害"));
        assert!(!copy.action_required);
        assert!(copy.summary.contains("メール本文の取得"));
    }

    #[test]
    fn test_fatal_alert_copy_contains_no_old_sentence() {
        // 古い文言「IMAP と決めつけないでください」が無いことを確認
        let copy = fatal_alert_copy(Phase1FatalKind::Other, ImapStage::Unknown);
        assert!(!copy.action.contains("IMAP と決めつけないでください"));
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
