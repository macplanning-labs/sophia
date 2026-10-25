/// domain/services/probe_monitor.rs — 外部向けトークンリンクへの「無効な試行」の連続を数える
///
/// `/token/{uuid}` `/invite/{uuid}` `/auth/portal-link/{token}` `/mobile-upload/{token}` は認証なしで開く。
/// APIキー認証の失敗(不明・失効・期限切れのキー)も、同じ仕組みで数える(DEMO-000181 項目6・7)。
/// トークンは UUID v4(推測は現実的でない)なので、
/// 守るべきは「当てられること」ではなく「探られていることに気づけること」。無効なトークンの試行を
/// IPごと・全体で数え、短時間に閾値を超えたときだけ、1回だけ知らせる(正常なら通知しない)。
/// DEMO-000167 項目6。
///
/// ## できないこと(明記)
/// - 制限(ブロック)はしない。取引先の事務所など、同じIPを複数人が使う場合に、正当な利用者を巻き込むため。
/// - IPは `presentation::auth_messages::client_ip` で決める。接続元が信用する中継(TRUSTED_PROXY_CIDRS)のときだけ
///   中継が書いた `X-Real-IP` / `X-Forwarded-For` の末尾を採用する(`CF-Connecting-IP` は見ない)。
///   信用しない接続元からの見出しは無視するので、外から偽装されない。
///   ただし nginx 経由で中継を信用しない構成では、IP が中継の IP にまとまって粗くなる。そのため IP別とは別に、全体の件数でも見る。
/// - カウンタはプロセスのメモリ上。再起動すると消える(長期の集計はログで行う)。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 数える時間窓
pub const WINDOW: Duration = Duration::from_secs(600);
/// 1つのIPが、窓の中で無効な試行をこの回数に達したら知らせる
pub const PER_IP_THRESHOLD: u32 = 10;
/// 全体(全IP合計)で、窓の中でこの回数に達したら知らせる(IPを変えながら探る場合)
pub const GLOBAL_THRESHOLD: u32 = 50;
/// 覚えておくキーの上限(メモリを使い切らないため)
const MAX_KEYS: usize = 10_000;

/// 全体の合計を数えるためのキー
pub const GLOBAL_KEY: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// 窓の中での、これまでの試行回数(今回を含む)
    pub count: u32,
    /// 今回、閾値に達した(この窓で1回だけ true になる)
    pub newly_alerted: bool,
}

#[derive(Debug)]
struct Bucket {
    window_start: Instant,
    count: u32,
    alerted: bool,
}

#[derive(Debug, Default)]
pub struct ProbeMonitor {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl ProbeMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// 試行を1件数える。窓を過ぎていたら数え直す。閾値に達した最初の1回だけ `newly_alerted` が true。
    pub fn record(&self, key: &str, threshold: u32, now: Instant) -> Outcome {
        // 他のスレッドが落ちて poisoned でも、数え続ける(数えそこなうより安全)
        let mut map = self.buckets.lock().unwrap_or_else(|e| e.into_inner());

        if map.len() >= MAX_KEYS && !map.contains_key(key) {
            map.retain(|_, b| now.duration_since(b.window_start) <= WINDOW);
            if map.len() >= MAX_KEYS {
                map.clear();
            }
        }

        let b = map.entry(key.to_string()).or_insert(Bucket { window_start: now, count: 0, alerted: false });
        if now.duration_since(b.window_start) > WINDOW {
            *b = Bucket { window_start: now, count: 0, alerted: false };
        }
        b.count += 1;
        let newly_alerted = b.count >= threshold && !b.alerted;
        if newly_alerted {
            b.alerted = true;
        }
        Outcome { count: b.count, newly_alerted }
    }
}

/// 外部向けトークンリンクのどの入口か
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Token,
    Invite,
    /// ポータルのマジックリンク(`/auth/portal-link/{token}`)
    PortalLink,
    /// スマホアップロード(`/mobile-upload/{token}`)
    MobileUpload,
    /// APIキー認証(パスでは分類しない。api_key_auth が直接記録する)
    ApiKey,
}

impl EntryKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::Invite => "invite",
            Self::PortalLink => "portal-link",
            Self::MobileUpload => "mobile-upload",
            Self::ApiKey => "api-key",
        }
    }

    /// 通知文で使う「何の無効な試行か」
    pub fn subject(&self) -> &'static str {
        match self {
            Self::ApiKey => "APIキー認証の失敗",
            _ => "無効なトークンの試行",
        }
    }
}

/// パスから、対象の入口と「トークン部分」を取り出す。対象外のパスは None。
/// 対象: `/token/{x}...` `/invite/{x}...` `/auth/portal-link/{x}...` `/mobile-upload/{x}...`(`/api/v1/` `/api/` 付きも)
pub fn classify(path: &str) -> Option<(EntryKind, &str)> {
    const PREFIXES: [(&str, EntryKind); 10] = [
        ("/api/v1/token/", EntryKind::Token),
        ("/api/token/", EntryKind::Token),
        ("/token/", EntryKind::Token),
        ("/api/v1/invite/", EntryKind::Invite),
        ("/api/invite/", EntryKind::Invite),
        ("/invite/", EntryKind::Invite),
        ("/api/v1/auth/portal-link/", EntryKind::PortalLink),
        ("/api/auth/portal-link/", EntryKind::PortalLink),
        ("/api/v1/mobile-upload/", EntryKind::MobileUpload),
        ("/api/mobile-upload/", EntryKind::MobileUpload),
    ];
    for (prefix, kind) in PREFIXES {
        if let Some(rest) = path.strip_prefix(prefix) {
            let seg = rest.split('/').next().unwrap_or("");
            return Some((kind, seg));
        }
    }
    None
}

/// 「無効なトークンの試行」か。UUID として読めない、または存在しない(404)なら試行とみなす。
/// 期限切れ(410)・使用済み・権限(403)・ファイルの不備(400)は、正規の利用者にも起こるので数えない。
pub fn is_invalid_probe(token_segment: &str, status: u16) -> bool {
    uuid::Uuid::parse_str(token_segment).is_err() || status == 404
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 閾値に達した最初の1回だけ通知する() {
        let m = ProbeMonitor::new();
        let t0 = Instant::now();
        for i in 1..PER_IP_THRESHOLD {
            let o = m.record("1.2.3.4", PER_IP_THRESHOLD, t0);
            assert_eq!(o, Outcome { count: i, newly_alerted: false });
        }
        let o = m.record("1.2.3.4", PER_IP_THRESHOLD, t0);
        assert_eq!(o, Outcome { count: PER_IP_THRESHOLD, newly_alerted: true });
        // その後は通知しない(連打しても通知が溢れない)
        let o = m.record("1.2.3.4", PER_IP_THRESHOLD, t0);
        assert_eq!(o, Outcome { count: PER_IP_THRESHOLD + 1, newly_alerted: false });
    }

    #[test]
    fn 窓を過ぎると数え直し_また通知できる() {
        let m = ProbeMonitor::new();
        let t0 = Instant::now();
        for _ in 0..PER_IP_THRESHOLD {
            m.record("a", PER_IP_THRESHOLD, t0);
        }
        let later = t0 + WINDOW + Duration::from_secs(1);
        let o = m.record("a", PER_IP_THRESHOLD, later);
        assert_eq!(o, Outcome { count: 1, newly_alerted: false });
        let mut last = o;
        for _ in 1..PER_IP_THRESHOLD {
            last = m.record("a", PER_IP_THRESHOLD, later);
        }
        assert!(last.newly_alerted);
    }

    #[test]
    fn ipごとに別々に数える() {
        let m = ProbeMonitor::new();
        let t0 = Instant::now();
        for _ in 0..(PER_IP_THRESHOLD - 1) {
            m.record("a", PER_IP_THRESHOLD, t0);
            m.record("b", PER_IP_THRESHOLD, t0);
        }
        // a は、あと1回で閾値。b も同じ件数だが、別に数えている
        assert!(m.record("a", PER_IP_THRESHOLD, t0).newly_alerted);
        assert!(m.record("b", PER_IP_THRESHOLD, t0).newly_alerted);
        assert_eq!(m.record("c", PER_IP_THRESHOLD, t0).count, 1);
    }

    #[test]
    fn 全体の合計でも通知できる() {
        let m = ProbeMonitor::new();
        let t0 = Instant::now();
        let mut alerted = false;
        for _ in 0..GLOBAL_THRESHOLD {
            alerted = m.record(GLOBAL_KEY, GLOBAL_THRESHOLD, t0).newly_alerted;
        }
        assert!(alerted);
    }

    #[test]
    fn キーが増えすぎても落ちず_上限を超えない() {
        let m = ProbeMonitor::new();
        let t0 = Instant::now();
        for i in 0..(MAX_KEYS + 500) {
            m.record(&format!("ip{i}"), PER_IP_THRESHOLD, t0);
        }
        assert!(m.buckets.lock().unwrap().len() <= MAX_KEYS);
    }

    #[test]
    fn 対象のパスだけ分類する() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(classify(&format!("/api/v1/token/{u}")), Some((EntryKind::Token, u)));
        assert_eq!(classify(&format!("/api/v1/token/{u}/accept")), Some((EntryKind::Token, u)));
        assert_eq!(classify(&format!("/token/{u}/pdf")), Some((EntryKind::Token, u)));
        assert_eq!(classify(&format!("/api/v1/invite/{u}/status")), Some((EntryKind::Invite, u)));
        assert_eq!(classify("/api/v1/token/abc"), Some((EntryKind::Token, "abc")));
        assert_eq!(classify("/api/v1/token/"), Some((EntryKind::Token, "")));
        // 対象外
        assert_eq!(classify("/api/v1/orders"), None);
        assert_eq!(classify("/api/v1/auth/portal-link"), None);
        assert_eq!(classify("/api/v1/auth/portal-links/x"), None);
        assert_eq!(classify("/api/v1/expenses/items/1/mobile-upload/token"), None);
        assert_eq!(classify("/tokens/x"), None);
        assert_eq!(classify("/"), None);
    }

    #[test]
    fn ポータルリンクとスマホアップロードも分類する() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(classify(&format!("/api/v1/auth/portal-link/{u}")), Some((EntryKind::PortalLink, u)));
        assert_eq!(classify(&format!("/api/v1/auth/portal-link/{u}/confirm")), Some((EntryKind::PortalLink, u)));
        assert_eq!(classify(&format!("/api/v1/mobile-upload/{u}")), Some((EntryKind::MobileUpload, u)));
        assert_eq!(classify(&format!("/api/mobile-upload/{u}/status")), Some((EntryKind::MobileUpload, u)));
        assert_eq!(classify("/api/v1/mobile-upload/abc"), Some((EntryKind::MobileUpload, "abc")));
        assert_eq!(EntryKind::PortalLink.label(), "portal-link");
        assert_eq!(EntryKind::MobileUpload.label(), "mobile-upload");
        assert_eq!(EntryKind::ApiKey.subject(), "APIキー認証の失敗");
    }

    #[test]
    fn 無効な試行の判定() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        // 存在しないトークン(404)
        assert!(is_invalid_probe(u, 404));
        // UUID として読めない(状態コードによらず)
        assert!(is_invalid_probe("abc", 200));
        assert!(is_invalid_probe("", 400));
        assert!(is_invalid_probe("../etc/passwd", 400));
        // 正規の利用者にも起こるもの(期限切れ・使用済み・権限・ファイルの不備・成功)は数えない
        for st in [200u16, 302, 400, 403, 410, 500] {
            assert!(!is_invalid_probe(u, st), "{st}");
        }
    }
}
