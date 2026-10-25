/// domain/services/chat_notifier.rs — 異常の通知（運用向け）への投稿（開発標準 §3.7）
///
/// メール(SMTP)経由のアラートはメールアカウント自体の認証情報が壊れていると
/// 一緒に送信不能になる(DEMO-000014)。Chat 等はメールアカウントの認証情報と
/// 無関係な別チャネルのため、致命的アラートはこちらも併用する。
///
/// 送り先は外部連携の部品（`integrations.toml` の `ops_alert`）で決まる（DEMO-000213）。
/// 送り先が未設定なら warn ログを残す。URL 等の秘密の値はログに出さない([SEC-13.2])。
///
/// 通知の形は開発標準 §3.7.2 にそろえる: 頭は `<絵文字>【Sophia】<レベル>`、本文は
/// 概要・影響範囲・発生時刻（日本時間）・次のアクション。環境は送り先のスペースで分かるので書かない。
/// 本文に秘密の値（パスワード・トークン・Webhook の URL）を入れない。
use chrono::{DateTime, FixedOffset, Utc};
use integration_core::{Level, Notice};

/// 通知に出すシステム名
pub const SYSTEM_NAME: &str = "Sophia";

/// 障害・運用の通知1件分
#[derive(Debug, Clone)]
pub struct Incident {
    pub level: Level,
    pub summary: String,
    pub impact: String,
    pub action: String,
    pub occurred_at: DateTime<Utc>,
}

impl Incident {
    /// 発生時刻は「いま」。始まりが分かっているときは [`Incident::at`] で直す
    pub fn new(level: Level, summary: impl Into<String>, impact: impl Into<String>, action: impl Into<String>) -> Self {
        Self { level, summary: summary.into(), impact: impact.into(), action: action.into(), occurred_at: Utc::now() }
    }

    pub fn at(mut self, t: DateTime<Utc>) -> Self {
        self.occurred_at = t;
        self
    }

    /// 人が動く通知か(Critical・High と、その復旧)。Warning・Info は動くことが無いので、運用の Chat には出さない。
    /// 理由: 動かなくてよい通知が混じると、見る人が減り、本当に動くべき通知に気づけなくなる(2026-10-12 佐藤さんの方針)。
    pub fn needs_human(&self) -> bool {
        matches!(self.level, Level::Critical | Level::High | Level::Recovered)
    }

    /// 日本時間の「YYYY-MM-DD HH:MM」
    fn occurred_jst(&self) -> String {
        match FixedOffset::east_opt(9 * 3600) {
            Some(tz) => self.occurred_at.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string(),
            None => self.occurred_at.format("%Y-%m-%d %H:%M(UTC)").to_string(),
        }
    }

    pub fn to_notice(&self) -> Notice {
        Notice::incident(SYSTEM_NAME, self.level, &self.summary, &self.impact, &self.occurred_jst(), &self.action)
    }

    /// 1つの文にしたもの(メールの本文など、Chat 以外の経路で同じ内容を送るとき)
    pub fn plain_text(&self) -> String {
        self.to_notice().to_plain_text()
    }

    /// メールの件名(頭の「絵文字【Sophia】レベル」に、概要の1行目を続ける)
    pub fn mail_subject(&self) -> String {
        let first = self.summary.lines().next().unwrap_or("");
        format!("{}【{}】{} {}", self.level.emoji(), SYSTEM_NAME, self.level.label(), first)
    }
}

/// 運用の Chat に送る。**人が動く通知(Critical・High・復旧)だけ**を送り、Warning・Info は送らない
/// (ログには残す。画面・メール・毎朝の点検で見られるようにする)。
pub async fn post(incident: &Incident) {
    if !incident.needs_human() {
        tracing::info!(
            "[通知] 運用の Chat には送りません(人が動く通知ではない: {}): {}",
            incident.level.label(),
            incident.summary.lines().next().unwrap_or("")
        );
        return;
    }
    crate::infrastructure::integrations::notify_ops(incident.to_notice()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_shape_with_jst_time() {
        let t = DateTime::parse_from_rfc3339("2026-10-12T00:15:00Z").unwrap().with_timezone(&Utc);
        let i = Incident::new(Level::High, "概要の文", "影響の文", "対応の文").at(t);
        assert_eq!(
            i.plain_text(),
            "🔥【Sophia】High\n概要：概要の文\n影響範囲：影響の文\n発生時刻：2026-10-12 09:15（日本時間）\n次のアクション：対応の文"
        );
        assert_eq!(i.mail_subject(), "🔥【Sophia】High 概要の文");
    }

    #[test]
    fn only_levels_that_need_a_human_go_to_chat() {
        let lv = |l| Incident::new(l, "a", "b", "c").needs_human();
        assert!(lv(Level::Critical));
        assert!(lv(Level::High));
        assert!(lv(Level::Recovered));
        assert!(!lv(Level::Warning));
        assert!(!lv(Level::Info));
    }

    #[test]
    fn recovered_uses_the_recovery_head() {
        let i = Incident::new(Level::Recovered, "直った", "なし", "不要");
        assert!(i.plain_text().starts_with("✅【Sophia】復旧\n"));
    }
}
