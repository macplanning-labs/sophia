/// domain/services/sso_monitor.rs — 社内 SSO の失敗を数え、続いたときだけ運用に知らせる判定(純粋な関数。DB・ネットワークに触れない)
///
/// 社内 SSO(Cloudflare Access の JWT でのログイン)の失敗のうち、次の 2 つを知らせる(DEMO-000254 B8)。
/// - 公開鍵の取得失敗: 公開鍵を取れず、社内アカウントでのログインが全員できない。10 分に 1 回。
/// - 拒否の急増: 不正な JWT・Service Token・アカウントなしなどの拒否が、IP 別 10 件、または全体で 50 件に、
///   10 分の窓で達したとき。窓ごとに 1 回だけ。
///
/// 数え方は `probe_monitor`(外部入口の無効な試行)と同じ。ブロックはしない(リクエストの扱いは変えない)。
/// 通知の文には、メール・JWT・Cookie の値を入れない(出してよいのは IP と件数だけ)。
/// カウンタはプロセスのメモリ上。再起動すると消える。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::domain::services::probe_monitor::{ProbeMonitor, GLOBAL_KEY, GLOBAL_THRESHOLD, PER_IP_THRESHOLD, WINDOW};

/// 公開鍵の取得失敗を数えるキー
const JWKS_KEY: &str = "jwks";

/// 通知にする出来事。値(メール・JWT)は持たない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsoAlert {
    /// 公開鍵を取得できない(社内アカウントでのログインが、すべて失敗する)
    JwksUnavailable,
    /// 公開鍵を取得できるようになった(`JwksUnavailable` を知らせたあとの 1 回だけ)
    JwksRecovered,
    /// 同じ IP からの拒否が続いている
    RejectionsFromIp { ip: String, count: u32 },
    /// 全体の拒否が急に増えた
    RejectionsGlobal { count: u32 },
}

#[derive(Debug, Default)]
pub struct SsoMonitor {
    rejections: ProbeMonitor,
    jwks: ProbeMonitor,
    /// `JwksUnavailable` を知らせたあと、まだ復旧を知らせていない
    jwks_alerted: AtomicBool,
}

impl SsoMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// 拒否を 1 件数える。IP 別が閾値に達した、全体が閾値に達した、それぞれ窓の中で最初の 1 回だけ、通知の種類を返す(両方なら 2 件)。
    pub fn record_rejection(&self, ip: &str, now: Instant) -> Vec<SsoAlert> {
        let mut alerts = Vec::new();
        let per_ip = self.rejections.record(ip, PER_IP_THRESHOLD, now);
        if per_ip.newly_alerted {
            alerts.push(SsoAlert::RejectionsFromIp { ip: ip.to_string(), count: per_ip.count });
        }
        let global = self.rejections.record(GLOBAL_KEY, GLOBAL_THRESHOLD, now);
        if global.newly_alerted {
            alerts.push(SsoAlert::RejectionsGlobal { count: global.count });
        }
        alerts
    }

    /// 公開鍵の取得失敗を数える。10 分の窓で最初の 1 回だけ Some(JwksUnavailable)。
    pub fn record_jwks_failure(&self, now: Instant) -> Option<SsoAlert> {
        if self.jwks.record(JWKS_KEY, 1, now).newly_alerted {
            self.jwks_alerted.store(true, Ordering::SeqCst);
            Some(SsoAlert::JwksUnavailable)
        } else {
            None
        }
    }

    /// 公開鍵を取得できた。失敗を知らせたあとなら、1 回だけ Some(JwksRecovered)。
    pub fn record_jwks_ok(&self) -> Option<SsoAlert> {
        if self.jwks_alerted.swap(false, Ordering::SeqCst) {
            Some(SsoAlert::JwksRecovered)
        } else {
            None
        }
    }
}

impl SsoAlert {
    /// 通知のレベル。復旧だけ Recovered、ほかは人が動く High(理由によらず調べる。DEMO-000332)
    pub fn level(&self) -> integration_core::Level {
        match self {
            Self::JwksRecovered => integration_core::Level::Recovered,
            _ => integration_core::Level::High,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::JwksUnavailable => "社内 SSO の公開鍵を取得できません（Cloudflare Access の JWT を検証できず、社内アカウントでのログインがすべて失敗しています）".to_string(),
            Self::JwksRecovered => "社内 SSO の公開鍵を取得できるようになりました".to_string(),
            Self::RejectionsFromIp { ip, count } => {
                format!("社内 SSO のログインの拒否が、同じ IP から続いています（IP={} {}件/{}分）", ip, count, WINDOW.as_secs() / 60)
            }
            Self::RejectionsGlobal { count } => {
                format!("社内 SSO のログインの拒否が、全体で{}件/{}分に達しました", count, WINDOW.as_secs() / 60)
            }
        }
    }

    pub fn impact(&self) -> &'static str {
        match self {
            Self::JwksUnavailable => "社内アカウントでのログインができません。パスワード・パスキーなど、従来のログインは使えます",
            Self::JwksRecovered => "なし（社内アカウントでのログインができる状態に戻りました）",
            Self::RejectionsFromIp { .. } | Self::RejectionsGlobal { .. } => {
                "アクセスはブロックしていません（IP 単位の回数制限は別にあります）。ログインできない人がいる、または不正な試行を受けている可能性があります。Cloudflare Access を通る入口に不正な値が届くのは、ふつうではありません（設定の変更の直後なら、AUD・チームドメインの誤りの可能性）"
            }
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::JwksUnavailable => "Cloudflare の状態と、Sophia の web から外への通信を確認してください。ログの [認証/SSO] を見てください",
            Self::JwksRecovered => "対応は不要です",
            Self::RejectionsFromIp { .. } | Self::RejectionsGlobal { .. } => {
                "監査ログ（h_auth_event）で、event_type が login_fail で detail が sso_ で始まる行を、時刻と ip で絞って見てください。detail の意味: sso_invalid_token=JWT が不正または検証に失敗、sso_no_token=JWT が無い、sso_service_token=Service Token での試行、sso_no_account=該当するアカウントが無い（ほかに sso_ambiguous_account・sso_not_eligible_role）"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn 拒否_ip別は_10件目で1回だけ通知する() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        for _ in 0..(PER_IP_THRESHOLD - 1) {
            assert!(m.record_rejection("1.2.3.4", t0).is_empty());
        }
        let a = m.record_rejection("1.2.3.4", t0);
        assert_eq!(a, vec![SsoAlert::RejectionsFromIp { ip: "1.2.3.4".into(), count: PER_IP_THRESHOLD }]);
        // 窓の中では、11 件目以降は通知しない
        assert!(m.record_rejection("1.2.3.4", t0).is_empty());
    }

    #[test]
    fn 拒否_全体は_50件目で通知する() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        let mut last = Vec::new();
        for i in 0..GLOBAL_THRESHOLD {
            // 異なる IP を使い、IP 別の通知が混ざらないようにする
            let ip = format!("10.0.0.{i}");
            let a = m.record_rejection(&ip, t0);
            if i + 1 < GLOBAL_THRESHOLD {
                assert!(!a.contains(&SsoAlert::RejectionsGlobal { count: i + 1 }));
            }
            last = a;
        }
        assert!(last.contains(&SsoAlert::RejectionsGlobal { count: GLOBAL_THRESHOLD }));
    }

    #[test]
    fn 窓を過ぎると数え直し_また通知できる() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        for _ in 0..PER_IP_THRESHOLD {
            m.record_rejection("a", t0);
        }
        let later = t0 + WINDOW + Duration::from_secs(1);
        let mut got = Vec::new();
        for _ in 0..PER_IP_THRESHOLD {
            got.extend(m.record_rejection("a", later));
        }
        assert_eq!(got, vec![SsoAlert::RejectionsFromIp { ip: "a".into(), count: PER_IP_THRESHOLD }]);
    }

    #[test]
    fn ipが違えば_別々に数える() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        for _ in 0..(PER_IP_THRESHOLD - 1) {
            assert!(m.record_rejection("ip-a", t0).is_empty());
            assert!(m.record_rejection("ip-b", t0).is_empty());
        }
        // どちらも 9 件(上のループで通知は出ていない)。あと 1 件で、IP A だけが閾値に達する(IP B は別に数える)
        assert_eq!(
            m.record_rejection("ip-a", t0),
            vec![SsoAlert::RejectionsFromIp { ip: "ip-a".into(), count: PER_IP_THRESHOLD }]
        );
        assert!(m.record_rejection("ip-b", t0).contains(&SsoAlert::RejectionsFromIp { ip: "ip-b".into(), count: PER_IP_THRESHOLD }));
    }

    #[test]
    fn jwks_の失敗は_窓の中で最初の1回だけ通知する() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        assert_eq!(m.record_jwks_failure(t0), Some(SsoAlert::JwksUnavailable));
        assert_eq!(m.record_jwks_failure(t0), None);
        let later = t0 + WINDOW + Duration::from_secs(1);
        assert_eq!(m.record_jwks_failure(later), Some(SsoAlert::JwksUnavailable));
    }

    #[test]
    fn 文に_値_メール_jwt_を含まない() {
        let alerts = [
            SsoAlert::JwksUnavailable,
            SsoAlert::JwksRecovered,
            SsoAlert::RejectionsFromIp { ip: "203.0.113.7".into(), count: 10 },
            SsoAlert::RejectionsGlobal { count: 50 },
        ];
        for a in alerts {
            for text in [a.summary(), a.impact().to_string(), a.action().to_string()] {
                assert!(!text.contains('@'), "{text}");
                assert!(!text.contains("eyJ"), "{text}");
                assert!(!text.contains("Bearer"), "{text}");
            }
        }
    }

    #[test]
    fn ip別の文に_ipと件数が入る() {
        let a = SsoAlert::RejectionsFromIp { ip: "203.0.113.7".into(), count: 10 };
        let s = a.summary();
        assert!(s.contains("203.0.113.7"));
        assert!(s.contains("10件"));
    }

    #[test]
    fn jwks_の復旧は_失敗を知らせたあとにだけ_1回出る() {
        let m = SsoMonitor::new();
        let t0 = Instant::now();
        // 失敗を知らせる前は、復旧は出ない
        assert_eq!(m.record_jwks_ok(), None);
        assert_eq!(m.record_jwks_failure(t0), Some(SsoAlert::JwksUnavailable));
        assert_eq!(m.record_jwks_ok(), Some(SsoAlert::JwksRecovered));
        // 2 回目は出ない
        assert_eq!(m.record_jwks_ok(), None);
    }

    #[test]
    fn レベル_復旧だけrecovered_ほかはhigh() {
        use integration_core::Level;
        assert_eq!(SsoAlert::JwksRecovered.level(), Level::Recovered);
        assert_eq!(SsoAlert::JwksUnavailable.level(), Level::High);
        assert_eq!(SsoAlert::RejectionsGlobal { count: 50 }.level(), Level::High);
    }

    #[test]
    fn 拒否の通知は_調べ方を含み_ログインの不具合とは書かない() {
        let a = SsoAlert::RejectionsFromIp { ip: "203.0.113.7".into(), count: 10 };
        assert!(a.action().contains("h_auth_event"));
        assert!(a.action().contains("sso_invalid_token"));
        assert!(!a.summary().contains("失敗が続いています"));
    }
}
