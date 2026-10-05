/// infrastructure/mail_pipeline/alert_policy.rs — 管理者通知の抑制・復旧通知・文面
///
/// 2026-10-03、Gmail 側の一時的な通信切断（自然に復旧）でも致命アラートが届き、
/// しかも原因分類を誤って「IMAP と決めつけないで」という分かりにくい文面になっていた。
/// 一時的な障害で何度も通知が届かないよう、次の方針で通知を出す。
///
/// - Gmail との通信障害（`ImapNetwork`）は一過性が多いので、`MAIL_PIPELINE_ALERT_NETWORK_THRESHOLD_RUNS`
///   回（既定 2 回＝約30分）連続で走査が失敗したときに初めて通知する。
/// - 認証失敗・設定漏れ・DB障害など、放置しても直らないものは初回で通知する。
/// - 通知後は、原因の種類が変わるか `MAIL_PIPELINE_ALERT_REPEAT_HOURS` 時間（既定 6 時間）
///   経つまで再通知しない。復旧したら復旧のお知らせを1回送る。
/// - 特定のメール1通だけが保存できない状態が `MAIL_PIPELINE_ALERT_MAIL_ERROR_RUNS` 回
///   （既定 4 回＝約1時間）続いたら、1回だけ通知する（2026-10-04、NUL文字を含むメールが
///   15分ごとに保存失敗し続けても誰にも通知されなかった）。
///
/// 状態はプロセス内に持つ（DB に持たない）。再起動でリセットされ、最悪でも
/// 「同じ障害の通知が1回重複する」「復旧のお知らせが1回届かない」程度で済むため許容する。
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Duration, Local};

use super::ops_message::{self, ImapStage, Phase1FatalKind};

/// 1回の試行の記録（通知本文の「試行履歴」に使う）
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub time: String,  // HH:MM:SS
    pub error: String, // 生のエラー文字列（ops_message::fail で包む前のもの）
}

/// 通知の設定（環境変数から読む）
pub struct AlertConfig {
    pub network_threshold_runs: u32,
    pub repeat_hours: i64,
    pub mail_error_runs: u32,
}

impl AlertConfig {
    pub fn from_env() -> Self {
        Self {
            network_threshold_runs: parse_positive(
                std::env::var("MAIL_PIPELINE_ALERT_NETWORK_THRESHOLD_RUNS").ok().as_deref(),
                2,
            ) as u32,
            repeat_hours: parse_positive(
                std::env::var("MAIL_PIPELINE_ALERT_REPEAT_HOURS").ok().as_deref(),
                6,
            ),
            mail_error_runs: parse_positive(
                std::env::var("MAIL_PIPELINE_ALERT_MAIL_ERROR_RUNS").ok().as_deref(),
                4,
            ) as u32,
        }
    }
}

/// 1以上の整数ならその値、それ以外（未設定・空・0・不正値）は既定値
fn parse_positive(raw: Option<&str>, default: i64) -> i64 {
    raw.and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(default)
}

/// 判定結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 通知する（連続失敗回数と最初に失敗した時刻つき）
    Alert { failed_runs: u32, since: DateTime<Local> },
    /// 通知しない（閾値未満、または通知済み）
    Suppress { failed_runs: u32 },
    /// 通知済みの障害が復旧した → 復旧のお知らせを送る
    Recovered { failed_runs: u32, since: DateTime<Local> },
    /// 通知前に自然に復旧した（ログのみ）
    SelfHealed { failed_runs: u32 },
    /// 何もなし
    Nothing,
}

/// Phase1 の連続失敗の状態
#[derive(Debug, Default)]
pub struct AlertState {
    consecutive_failed_runs: u32,
    first_failed_at: Option<DateTime<Local>>,
    last_alerted_at: Option<DateTime<Local>>,
    last_kind: Option<Phase1FatalKind>,
}

impl AlertState {
    /// Phase1 が失敗した走査1回ぶんを記録し、通知するかを決める
    pub fn on_failure(&mut self, kind: Phase1FatalKind, now: DateTime<Local>, cfg: &AlertConfig) -> Decision {
        if self.consecutive_failed_runs == 0 {
            self.first_failed_at = Some(now);
        }
        self.consecutive_failed_runs += 1;
        let kind_changed = self.last_kind.is_some_and(|k| k != kind);
        self.last_kind = Some(kind);

        let failed_runs = self.consecutive_failed_runs;
        let threshold = if kind == Phase1FatalKind::ImapNetwork {
            cfg.network_threshold_runs
        } else {
            1
        };
        if failed_runs < threshold {
            return Decision::Suppress { failed_runs };
        }

        let should_alert = match self.last_alerted_at {
            None => true,
            Some(last) => kind_changed || now - last >= Duration::hours(cfg.repeat_hours),
        };
        if !should_alert {
            return Decision::Suppress { failed_runs };
        }
        self.last_alerted_at = Some(now);
        Decision::Alert {
            failed_runs,
            since: self.first_failed_at.unwrap_or(now),
        }
    }

    /// Phase1 が成功した。通知済みなら復旧のお知らせを送る
    pub fn on_success(&mut self) -> Decision {
        let failed_runs = self.consecutive_failed_runs;
        let since = self.first_failed_at;
        let alerted = self.last_alerted_at.is_some();
        *self = Self::default();

        match (failed_runs, alerted, since) {
            (0, _, _) => Decision::Nothing,
            (_, true, Some(since)) => Decision::Recovered { failed_runs, since },
            _ => Decision::SelfHealed { failed_runs },
        }
    }
}

/// 特定のメールだけが保存できない状態の追跡（キー = エラー1行。message_id を含む）
#[derive(Debug, Default)]
pub struct MailErrorTracker {
    // エラー1行 → (連続して出た走査回数, 通知済みか)
    entries: HashMap<String, (u32, bool)>,
}

impl MailErrorTracker {
    /// 走査1回分の個別エラーを記録し、今回はじめて閾値に達したもの（＝通知すべきもの）を返す。
    /// 今回出なかったエラーは忘れる（再び出たら1回目から数え直す）。
    pub fn observe_run(&mut self, errors: &[String], threshold_runs: u32) -> Vec<String> {
        self.entries.retain(|k, _| errors.contains(k));
        let mut to_alert = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for e in errors {
            if !seen.insert(e) {
                continue;
            }
            let entry = self.entries.entry(e.clone()).or_insert((0, false));
            entry.0 += 1;
            if entry.0 >= threshold_runs && !entry.1 {
                entry.1 = true;
                to_alert.push(e.clone());
            }
        }
        to_alert
    }
}

static STATE: OnceLock<Mutex<AlertState>> = OnceLock::new();
static MAIL_ERRORS: OnceLock<Mutex<MailErrorTracker>> = OnceLock::new();

/// プロセス内の状態で on_failure を実行する
pub fn record_failure(kind: Phase1FatalKind, now: DateTime<Local>, cfg: &AlertConfig) -> Decision {
    let mut state = STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state.on_failure(kind, now, cfg)
}

/// プロセス内の状態で on_success を実行する
pub fn record_success() -> Decision {
    let mut state = STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state.on_success()
}

/// プロセス内の状態で個別メールのエラーを記録する
pub fn record_mail_errors(errors: &[String], cfg: &AlertConfig) -> Vec<String> {
    let mut tracker = MAIL_ERRORS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    tracker.observe_run(errors, cfg.mail_error_runs)
}

/// 管理者へ送る通知の文面（Chat とメールで共通の内容）
#[derive(Debug)]
pub struct AlertMessage {
    pub subject: String,
    pub body: String,
    pub chat: String,
}

const IMPACT_TEXT: &str = "新着メールの取込が遅れています。取込済みのメールへの影響はありません。\
復旧後、停止中に届いたメールも自動で取り込まれます（毎回の走査で3日前までさかのぼって読み直すため）。";

/// Phase1 が止まったときの通知文面
pub fn build_fatal_alert(
    raw_error: &str,
    attempts: &[AttemptRecord],
    failed_runs: u32,
    since: DateTime<Local>,
    now: DateTime<Local>,
) -> AlertMessage {
    let kind = ops_message::classify_phase1_fatal(raw_error);
    let stage = ops_message::imap_stage(raw_error);
    let copy = ops_message::fatal_alert_copy(kind, stage);
    let tag = if copy.action_required {
        "【要対応】"
    } else {
        "【対応不要（自動で再試行中）】"
    };
    let now_str = now.format("%Y-%m-%d %H:%M");
    let duration = if failed_runs > 1 {
        format!(
            "{} から、{}回の走査で続けて失敗しています。",
            since.format("%m/%d %H:%M"),
            failed_runs
        )
    } else {
        "今回の走査で失敗しました。".to_string()
    };
    let history: String = attempts
        .iter()
        .enumerate()
        .map(|(i, a)| format!("  {}回目 {} {}\n", i + 1, a.time, a.error))
        .collect();
    let stage_name = if stage == ImapStage::Unknown { "-" } else { stage.name_japanese() };

    let body = format!(
        "管理者各位\n\n{tag}\n\n\
         ■ 何が起きたか\n{summary}\n{duration}\n\n\
         ■ 考えられる原因\n{cause}\n\n\
         ■ 必要な対応\n{action}\n\n\
         ■ 影響\n{IMPACT_TEXT}\n\n\
         ■ 詳細（開発者向け）\n原因分類: {kind:?} / 段階: {stage_name}\n今回の試行履歴:\n{history}",
        summary = copy.summary,
        cause = copy.likely_cause,
        action = copy.action,
    );
    let last_error = attempts.last().map(|a| a.error.as_str()).unwrap_or(raw_error);
    let chat = format!(
        "*【Sophia】{title} ({now_str})*\n{tag}\n{summary}\n{duration}\n\n必要な対応: {action}\n\n詳細: {last_error}",
        title = copy.title,
        summary = copy.summary,
        action = copy.action,
    );
    AlertMessage {
        subject: format!("【Sophia】{tag}{} ({now_str})", copy.title),
        body,
        chat,
    }
}

/// 通知済みの障害が復旧したときの文面
pub fn build_recovery_notice(failed_runs: u32, since: DateTime<Local>, now: DateTime<Local>) -> AlertMessage {
    let now_str = now.format("%Y-%m-%d %H:%M");
    let minutes = (now - since).num_minutes();
    let text = format!(
        "メール自動取込が復旧しました。\n\
         停止していた期間: {} 〜 {}（約{minutes}分、{failed_runs}回の走査で失敗）\n\n\
         停止中に届いたメールは、毎回の走査で3日前までさかのぼって読み直すため、自動で取り込まれます。対応は不要です。",
        since.format("%m/%d %H:%M"),
        now.format("%m/%d %H:%M"),
    );
    AlertMessage {
        subject: format!("【Sophia】【復旧】メール自動取込が復旧しました ({now_str})"),
        body: format!("管理者各位\n\n{text}\n"),
        chat: format!("*【Sophia】メール自動取込が復旧しました ({now_str})*\n{text}"),
    }
}

/// 特定のメールだけが取り込めない状態が続くときの文面
pub fn build_mail_error_alert(error_line: &str, runs: u32, now: DateTime<Local>) -> AlertMessage {
    let now_str = now.format("%Y-%m-%d %H:%M");
    let text = format!(
        "【要対応】特定のメール1通が取り込めない状態が続いています（{runs}回の走査で続けて失敗）。\n\
         他のメールは正常に取り込まれています。このメールだけ、取込が止まっています。\n\
         開発者に、下記の詳細を連絡してください（同じ内容ではこれ以上通知しません）。\n\n\
         詳細: {error_line}"
    );
    AlertMessage {
        subject: format!("【Sophia】【要対応】特定のメールが取り込めない状態が続いています ({now_str})"),
        body: format!("管理者各位\n\n{text}\n"),
        chat: format!("*【Sophia】特定のメールが取り込めない状態が続いています ({now_str})*\n{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AlertConfig {
        AlertConfig { network_threshold_runs: 2, repeat_hours: 6, mail_error_runs: 4 }
    }

    fn t0() -> DateTime<Local> {
        Local::now()
    }

    #[test]
    fn parse_positive_falls_back_to_default() {
        assert_eq!(parse_positive(None, 2), 2);
        assert_eq!(parse_positive(Some(""), 2), 2);
        assert_eq!(parse_positive(Some("0"), 2), 2);
        assert_eq!(parse_positive(Some("abc"), 2), 2);
        assert_eq!(parse_positive(Some("3"), 2), 3);
    }

    #[test]
    fn network_failure_alerts_from_second_run_then_recovers() {
        let mut s = AlertState::default();
        let t = t0();
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t, &cfg()),
            Decision::Suppress { failed_runs: 1 }
        );
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t + Duration::minutes(15), &cfg()),
            Decision::Alert { failed_runs: 2, since: t }
        );
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t + Duration::minutes(30), &cfg()),
            Decision::Suppress { failed_runs: 3 }
        );
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t + Duration::hours(7), &cfg()),
            Decision::Alert { failed_runs: 4, since: t }
        );
        assert_eq!(s.on_success(), Decision::Recovered { failed_runs: 4, since: t });
        assert_eq!(s.on_success(), Decision::Nothing);
    }

    #[test]
    fn single_network_failure_self_heals_without_alert() {
        // 2026-10-03 の事象（1回だけ失敗し、次の走査で復旧）は通知しない
        let mut s = AlertState::default();
        assert!(matches!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t0(), &cfg()),
            Decision::Suppress { .. }
        ));
        assert_eq!(s.on_success(), Decision::SelfHealed { failed_runs: 1 });
    }

    #[test]
    fn auth_failure_alerts_immediately_and_once() {
        let mut s = AlertState::default();
        let t = t0();
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapAuth, t, &cfg()),
            Decision::Alert { failed_runs: 1, since: t }
        );
        assert!(matches!(
            s.on_failure(Phase1FatalKind::ImapAuth, t + Duration::minutes(15), &cfg()),
            Decision::Suppress { .. }
        ));
    }

    #[test]
    fn kind_change_after_alert_alerts_again_and_recovery_is_still_sent() {
        let mut s = AlertState::default();
        let t = t0();
        assert!(matches!(s.on_failure(Phase1FatalKind::Database, t, &cfg()), Decision::Alert { .. }));
        // 種類が変わったら（通信障害でも、連続失敗が閾値以上なので）改めて通知する
        assert_eq!(
            s.on_failure(Phase1FatalKind::ImapNetwork, t + Duration::minutes(15), &cfg()),
            Decision::Alert { failed_runs: 2, since: t }
        );
        assert_eq!(s.on_success(), Decision::Recovered { failed_runs: 2, since: t });
    }

    #[test]
    fn mail_error_alerts_once_after_threshold_and_resets_when_gone() {
        let mut m = MailErrorTracker::default();
        let e = vec!["[メール取込/Phase1] ... message_id=a@google.com | DBエラー code=22021".to_string()];
        assert!(m.observe_run(&e, 4).is_empty());
        assert!(m.observe_run(&e, 4).is_empty());
        assert!(m.observe_run(&e, 4).is_empty());
        assert_eq!(m.observe_run(&e, 4), e);
        assert!(m.observe_run(&e, 4).is_empty(), "通知は1回だけ");
        assert!(m.observe_run(&[], 4).is_empty());
        assert!(m.observe_run(&e, 4).is_empty(), "一度消えたら1回目から数え直す");
    }

    #[test]
    fn fatal_alert_for_connection_lost_says_no_action_needed() {
        let t = t0();
        let attempts = vec![
            AttemptRecord { time: "16:41:02".into(), error: "IMAP接続エラー(TCP): timed out".into() },
            AttemptRecord { time: "16:46:05".into(), error: "IMAP取得エラー(FETCH): Connection Lost".into() },
        ];
        let msg = build_fatal_alert(
            "IMAP取得エラー(FETCH): Connection Lost",
            &attempts,
            2,
            t,
            t + Duration::minutes(15),
        );
        assert!(msg.subject.contains("【対応不要（自動で再試行中）】"));
        assert!(msg.body.contains("メール本文の取得"));
        assert!(msg.body.contains("2回目 16:46:05 IMAP取得エラー(FETCH): Connection Lost"));
        assert!(!msg.body.contains("決めつけ"));
        assert!(msg.chat.contains("Connection Lost"));
    }

    #[test]
    fn fatal_alert_for_auth_failure_says_action_required() {
        let raw = "IMAPログインエラー: No Response: [AUTHENTICATIONFAILED] Invalid credentials (Failure)";
        let msg = build_fatal_alert(raw, &[], 1, t0(), t0());
        assert!(msg.subject.contains("【要対応】"));
    }

    #[test]
    fn fatal_alert_for_database_is_not_reported_as_gmail() {
        let raw = "自社情報のSMTP認証情報取得に失敗しました: DBエラー: error communicating with database: Connection reset by peer (os error 104)";
        let msg = build_fatal_alert(raw, &[], 1, t0(), t0());
        assert!(msg.subject.contains("データベース"));
        assert!(!msg.subject.contains("Gmail"));
    }

    #[test]
    fn recovery_notice_says_mails_are_not_lost() {
        let t = t0();
        let msg = build_recovery_notice(3, t, t + Duration::minutes(45));
        assert!(msg.chat.contains("自動で取り込まれます"));
        assert!(msg.body.contains("約45分"));
        assert!(!msg.chat.contains("取り込まれません"));
    }
}
