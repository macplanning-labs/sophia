//! 「知らせる」役割と、複数の送り先へ一斉に送る CompositeNotifier

use std::sync::Arc;

use async_trait::async_trait;

use crate::check::{CheckReport, Integration};

/// 知らせの重さ(開発標準 §3.7.3 の障害レベル)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// 🚨 サービス全停止・重大なデータ破損・セキュリティ侵害など。即時対応が必要
    Critical,
    /// 🔥 主要機能の一部停止。代わりの手段はある
    High,
    /// ⚠️ 軽微なエラーの多発・滞留・一時的な失敗の繰り返し
    Warning,
    /// ℹ️ 定期ジョブの成功・デプロイの完了・テスト送信など。送り過ぎない
    Info,
    /// ✅ 障害の通知を出したものが直った
    Recovered,
}

impl Level {
    /// 通知の頭に出す英語のレベル名(標準の表記。復旧だけ日本語)
    pub fn label(self) -> &'static str {
        match self {
            Level::Critical => "Critical",
            Level::High => "High",
            Level::Warning => "Warning",
            Level::Info => "Info",
            Level::Recovered => "復旧",
        }
    }

    /// 通知の頭に出す絵文字
    pub fn emoji(self) -> &'static str {
        match self {
            Level::Critical => "🚨",
            Level::High => "🔥",
            Level::Warning => "⚠️",
            Level::Info => "ℹ️",
            Level::Recovered => "✅",
        }
    }

    /// 画面・ログに出す日本語の名前
    pub fn label_ja(self) -> &'static str {
        match self {
            Level::Critical => "緊急",
            Level::High => "重大",
            Level::Warning => "注意",
            Level::Info => "お知らせ",
            Level::Recovered => "復旧",
        }
    }
}

/// 知らせる内容
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: Level,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
}

impl Notice {
    /// 既存の「本文だけ」の通知を包む(タイトルなし。送り先がレベルの絵文字を頭に付ける)
    pub fn text(level: Level, body: impl Into<String>) -> Self {
        Self { level, title: String::new(), body: body.into(), link: None }
    }

    /// 開発標準 §3.7.2 の形の通知を組み立てる。
    ///
    /// ```text
    /// 🔥【Sophia】High
    /// 概要：…
    /// 影響範囲：…
    /// 発生時刻：YYYY-MM-DD HH:MM（日本時間）
    /// 次のアクション：…
    /// ```
    /// - `system` はシステム名(例: "Sophia")。環境は送り先のスペースで分かるので、本文には入れない。
    /// - `occurred_at` は日本時間の「YYYY-MM-DD HH:MM」(呼ぶ側で整える)。
    /// - 秘密の値(パスワード・トークン・Webhook の URL・個人情報)は、どの項目にも入れない。
    pub fn incident(
        system: &str,
        level: Level,
        summary: &str,
        impact: &str,
        occurred_at: &str,
        action: &str,
    ) -> Self {
        Self {
            level,
            title: format!("{}【{}】{}", level.emoji(), system, level.label()),
            body: format!(
                "概要：{summary}\n影響範囲：{impact}\n発生時刻：{occurred_at}（日本時間）\n次のアクション：{action}"
            ),
            link: None,
        }
    }

    /// タイトル・本文・リンクを1つの文にする(Chat など、文1つしか送れない送り先用)
    pub fn to_plain_text(&self) -> String {
        let mut s = String::new();
        if !self.title.is_empty() {
            s.push_str(&self.title);
            s.push('\n');
        }
        s.push_str(&self.body);
        if let Some(link) = &self.link {
            s.push('\n');
            s.push_str(link);
        }
        s
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("送り先がエラーを返しました: {0}")]
    Remote(String),
    #[error("送り先と通信できません: {0}")]
    Transport(String),
    #[error("送り先の設定が正しくありません: {0}")]
    Config(String),
    /// CompositeNotifier で、すべての送り先に届かなかった
    #[error("すべての送り先に届きませんでした({0}件)")]
    AllFailed(usize),
    #[error("送り先が1つもありません")]
    NoTarget,
}

/// 知らせる
#[async_trait]
pub trait Notifier: Integration {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError>;
}

/// 送り先1つ分の送信結果
#[derive(Debug)]
pub struct Delivery {
    /// 送り先の名前(アプリが付けたもの。例: 「運用チームの Chat」)
    pub label: String,
    pub provider: &'static str,
    pub result: Result<(), NotifyError>,
}

/// 複数の送り先を持ち、全部へ送る。
///
/// - 1つでも届けば成功([`Notifier::notify`] は `Ok`)。
/// - 送り先ごとの結果は [`CompositeNotifier::notify_all`] で受け取れる(届かなかった送り先の記録用)。
pub struct CompositeNotifier {
    targets: Vec<(String, Arc<dyn Notifier>)>,
}

impl CompositeNotifier {
    pub fn new() -> Self {
        Self { targets: Vec::new() }
    }

    pub fn with(mut self, label: impl Into<String>, target: Arc<dyn Notifier>) -> Self {
        self.targets.push((label.into(), target));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// 送り先の名前の一覧
    pub fn labels(&self) -> Vec<&str> {
        self.targets.iter().map(|(l, _)| l.as_str()).collect()
    }

    /// 全部の送り先へ送り、送り先ごとの結果を返す(1つ失敗しても他へは送る)
    pub async fn notify_all(&self, notice: &Notice) -> Vec<Delivery> {
        let mut out = Vec::with_capacity(self.targets.len());
        for (label, target) in &self.targets {
            let result = target.notify(notice).await;
            out.push(Delivery { label: label.clone(), provider: target.provider(), result });
        }
        out
    }

    /// 送り先ごとの点検結果
    pub async fn check_all(&self) -> Vec<(String, &'static str, CheckReport)> {
        let mut out = Vec::with_capacity(self.targets.len());
        for (label, target) in &self.targets {
            out.push((label.clone(), target.provider(), target.check().await));
        }
        out
    }
}

impl Default for CompositeNotifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Integration for CompositeNotifier {
    fn provider(&self) -> &'static str {
        "composite"
    }

    /// 全送り先の点検をまとめる(項目名の頭に送り先の名前を付ける)
    async fn check(&self) -> CheckReport {
        if self.targets.is_empty() {
            return CheckReport::new().fail("送り先", "1つも設定されていません");
        }
        let mut report = CheckReport::new();
        for (label, _provider, r) in self.check_all().await {
            for item in r.items {
                report.items.push(crate::CheckItem {
                    name: format!("{label}: {}", item.name),
                    ok: item.ok,
                    detail: item.detail,
                });
            }
        }
        report
    }
}

#[async_trait]
impl Notifier for CompositeNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        if self.targets.is_empty() {
            return Err(NotifyError::NoTarget);
        }
        let results = self.notify_all(notice).await;
        let failed = results.iter().filter(|d| d.result.is_err()).count();
        if failed == results.len() {
            Err(NotifyError::AllFailed(failed))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        fail: bool,
        sent: AtomicUsize,
    }

    #[async_trait]
    impl Integration for Fake {
        fn provider(&self) -> &'static str {
            "fake"
        }
        async fn check(&self) -> CheckReport {
            if self.fail {
                CheckReport::new().fail("形", "NG")
            } else {
                CheckReport::new().pass("形")
            }
        }
    }

    #[async_trait]
    impl Notifier for Fake {
        async fn notify(&self, _n: &Notice) -> Result<(), NotifyError> {
            self.sent.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(NotifyError::Remote("HTTP 500".into()))
            } else {
                Ok(())
            }
        }
    }

    fn fake(fail: bool) -> Arc<Fake> {
        Arc::new(Fake { fail, sent: AtomicUsize::new(0) })
    }

    #[tokio::test]
    async fn sends_to_all_even_if_one_fails() {
        let a = fake(true);
        let b = fake(false);
        let c = CompositeNotifier::new().with("A", a.clone()).with("B", b.clone());
        let n = Notice::text(Level::High, "本文");
        let results = c.notify_all(&n).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].result.is_err());
        assert!(results[1].result.is_ok());
        assert_eq!(a.sent.load(Ordering::SeqCst), 1);
        assert_eq!(b.sent.load(Ordering::SeqCst), 1);
        assert!(c.notify(&n).await.is_ok(), "1つでも届けば成功");
    }

    #[tokio::test]
    async fn all_failed_is_error() {
        let c = CompositeNotifier::new().with("A", fake(true)).with("B", fake(true));
        let r = c.notify(&Notice::text(Level::High, "x")).await;
        assert!(matches!(r, Err(NotifyError::AllFailed(2))));
    }

    #[tokio::test]
    async fn empty_is_error_and_check_ng() {
        let c = CompositeNotifier::new();
        assert!(matches!(c.notify(&Notice::text(Level::Info, "x")).await, Err(NotifyError::NoTarget)));
        assert!(!c.check().await.ok());
    }

    #[tokio::test]
    async fn check_prefixes_label() {
        let c = CompositeNotifier::new().with("運用", fake(false)).with("開発", fake(true));
        let r = c.check().await;
        assert!(!r.ok());
        assert_eq!(r.failures(), vec!["開発: 形: NG".to_string()]);
    }

    #[test]
    fn incident_has_the_standard_shape() {
        let n = Notice::incident("Sophia", Level::High, "メール取込が3回続けて失敗しています", "注文書・勤務表の取り込みが止まっています", "2026-10-11 09:15", "IMAP の接続設定を確認してください");
        assert_eq!(n.title, "🔥【Sophia】High");
        assert_eq!(
            n.body,
            "概要：メール取込が3回続けて失敗しています\n影響範囲：注文書・勤務表の取り込みが止まっています\n発生時刻：2026-10-11 09:15（日本時間）\n次のアクション：IMAP の接続設定を確認してください"
        );
        assert!(n.to_plain_text().starts_with("🔥【Sophia】High\n概要："));
    }

    #[test]
    fn levels_match_the_standard() {
        let all = [
            (Level::Critical, "🚨", "Critical"),
            (Level::High, "🔥", "High"),
            (Level::Warning, "⚠️", "Warning"),
            (Level::Info, "ℹ️", "Info"),
            (Level::Recovered, "✅", "復旧"),
        ];
        for (l, e, name) in all {
            assert_eq!(l.emoji(), e);
            assert_eq!(l.label(), name);
        }
        assert_eq!(Notice::incident("S", Level::Recovered, "a", "b", "t", "c").title, "✅【S】復旧");
    }

    #[test]
    fn plain_text() {
        let n = Notice { level: Level::Warning, title: "題".into(), body: "本文".into(), link: Some("https://x".into()) };
        assert_eq!(n.to_plain_text(), "題\n本文\nhttps://x");
        assert_eq!(Notice::text(Level::Info, "本文").to_plain_text(), "本文");
    }
}
