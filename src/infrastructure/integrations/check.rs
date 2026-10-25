//! 外部連携の点検（起動時・手動の `sophia check-integrations`）
//!
//! 一覧（integrations.toml）の用途ごとに状態を調べ、`integration_core::evaluate` で照らし合わせる。
//! 点検は相手に何も書き込まない（Drive にファイルを作らない、Chat に送らない）。

use std::collections::HashMap;

use integration_core::{evaluate, CheckReport, Integration, Level, UseReport, UseStatus};
use sqlx::PgPool;

use super::registry;
use crate::domain::services::chat_notifier::Incident;
use crate::infrastructure::repositories::{integration_repo, issued_document_repo};

/// この点検が調べ方を知っている用途（一覧に足したら、ここと `status_of` にも足す）
pub const HANDLED_USES: &[&str] = &["ops_alert", "issued_pdf_archive", "received_doc_archive", "mail_send", "db_backup"];

fn from_report(r: CheckReport) -> UseStatus {
    if r.ok() {
        UseStatus::Ok
    } else {
        UseStatus::Failed(r.failures())
    }
}

/// メール送信: Gmail API の鍵のファイルと、送信元のアドレスがあるか（段階1では「設定がある」ことだけ）
async fn mail_send_status(pool: &PgPool) -> UseStatus {
    let mut missing = Vec::new();
    match std::env::var("GOOGLE_MAIL_CREDENTIALS_FILE").ok().filter(|v| !v.trim().is_empty()) {
        None => missing.push("GOOGLE_MAIL_CREDENTIALS_FILE が設定されていません".to_string()),
        Some(p) if !std::path::Path::new(&p).exists() => {
            missing.push("GOOGLE_MAIL_CREDENTIALS_FILE のファイルがありません".to_string())
        }
        Some(_) => {}
    }
    if crate::domain::services::email_service::get_notify_email(pool).await.trim().is_empty() {
        missing.push("送信元のアドレス（自社情報 または DEFAULT_FROM_EMAIL）が設定されていません".to_string());
    }
    if missing.is_empty() {
        UseStatus::Ok
    } else {
        UseStatus::Missing(missing.join(" / "))
    }
}

/// メール送信の設定に不足があれば、画面に出す文言を返す(送る前の確認用。不足が無ければ None)。
/// 設定が無い環境では、送信を試みても必ず失敗し、画面には「サーバーエラー」としか出ないため、先に断る。
pub async fn mail_setup_problem(pool: &PgPool) -> Option<String> {
    match mail_send_status(pool).await {
        UseStatus::Ok => None,
        _ => Some("メールの設定が完了していないため、送信できません。管理者に、設定 > 外部連携 の確認を依頼してください".to_string()),
    }
}

/// 送り先1つの点検結果を記録する(画面の設定は DB、サーバの設定ファイルは控え)
async fn record_target_check(pool: &PgPool, record: bool, use_id: &str, id: Option<i64>, label: &str, r: &CheckReport) {
    if !record {
        return;
    }
    let detail: Vec<String> = r
        .items
        .iter()
        .map(|i| match (&i.ok, &i.detail) {
            (true, _) => format!("OK: {}", i.name),
            (false, Some(d)) => format!("NG: {}: {}", i.name, d),
            (false, None) => format!("NG: {}", i.name),
        })
        .collect();
    let snap = super::Snapshot { at: chrono::Utc::now(), ok: r.ok(), detail: detail.clone() };
    match id {
        Some(id) => {
            if let Err(e) = integration_repo::record_check(pool, id, r.ok(), &serde_json::json!(detail)).await {
                tracing::warn!("[外部連携の点検] 点検の結果を記録できません({label}): {e}");
            }
        }
        None => super::put_snapshot(super::snapshot_key(use_id, label), snap),
    }
}

/// 用途ごとの状態を調べる(送り先ごとに点検する)。`record` が true のときだけ、結果を DB・控えに記録する
/// (手動の `sophia check-integrations` とデプロイ前の点検は false: 本番の DB に何も書かない)
pub async fn collect_statuses(pool: &PgPool, record: bool) -> HashMap<String, UseStatus> {
    let reg = registry();
    let mut st = HashMap::new();

    // 通知: 全部の送り先を点検する。1つでも使えなければ「異常」(使える送り先が1つも無ければ「未設定」)
    let mut fails: Vec<String> = reg.notify_broken.iter().map(|b| format!("{}: {}", b.label, b.reason)).collect();
    for t in &reg.notify {
        let r = t.notifier.check().await;
        record_target_check(pool, record, super::NOTIFY_USE, t.id, &t.label, &r).await;
        fails.extend(r.failures().into_iter().map(|f| format!("{}: {f}", t.label)));
    }
    let ops = if reg.notify.is_empty() {
        let why = reg.ops_alert().err().unwrap_or("送り先がありません").to_string();
        if fails.is_empty() { UseStatus::Missing(why) } else { UseStatus::Failed(fails) }
    } else if fails.is_empty() {
        UseStatus::Ok
    } else {
        UseStatus::Failed(fails)
    };
    st.insert(super::NOTIFY_USE.to_string(), ops);

    // 保存先: 発行した PDF も受信メールの添付も、同じ保存先の設定を使う
    let store = match (&reg.store, &reg.store_broken) {
        (Some(t), _) => {
            let r = t.store.check().await;
            record_target_check(pool, record, super::STORE_USE, t.id, &t.label, &r).await;
            // 画面に「どのフォルダか」を名前で出すため、見えるときはフォルダ名を控える(書き込みなし)
            if r.ok() {
                match t.store.folder_name().await {
                    Ok(name) if !name.is_empty() => super::put_folder_name(&t.destination, name),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("[外部連携の点検] フォルダ名を取れません: {e}"),
                }
            }
            from_report(r)
        }
        (None, Some(b)) => UseStatus::Failed(vec![format!("{}: {}", b.label, b.reason)]),
        (None, None) => UseStatus::Missing(reg.file_store().err().unwrap_or("保存先がありません").to_string()),
    };
    st.insert(super::STORE_USE.to_string(), store.clone());
    st.insert("received_doc_archive".to_string(), store);

    st.insert("mail_send".to_string(), mail_send_status(pool).await);

    st.insert(
        "db_backup".to_string(),
        UseStatus::Deferred("サーバの仕事のため、本番デプロイ前の点検で確かめる".to_string()),
    );
    st
}

/// 一覧と照らし合わせた結果。全体の結果も控える(画面の「最後の点検」)
pub async fn run(pool: &PgPool, record: bool) -> Vec<UseReport> {
    let statuses = collect_statuses(pool, record).await;
    let reports = evaluate(&registry().manifest, &statuses);
    if record {
        let problems: Vec<String> = reports.iter().filter(|r| r.is_problem()).map(plain_problem).collect();
        super::put_snapshot(
            OVERALL_KEY.to_string(),
            super::Snapshot { at: chrono::Utc::now(), ok: problems.is_empty(), detail: problems },
        );
    }
    reports
}

/// 全体の点検結果の控えのキー
pub const OVERALL_KEY: &str = "__overall__";

/// 起動時の点検: 結果をログに残し、問題があれば知らせる（止めない。設計 D5）
pub async fn run_at_startup(pool: PgPool) {
    let reports = run(&pool, true).await;
    let problems: Vec<&UseReport> = reports.iter().filter(|r| r.is_problem()).collect();
    for r in &reports {
        if r.is_problem() {
            tracing::error!("[外部連携の点検] {}", r.summary_ja());
        } else {
            tracing::info!("[外部連携の点検] {}", r.summary_ja());
        }
    }
    if problems.is_empty() {
        tracing::info!("[外部連携の点検] 必須の連携はすべて使えます");
        return;
    }

    let incident = Incident::new(
        Level::High,
        format!("起動時の点検で、使えない外部連携が見つかりました（{}件）", problems.len()),
        format!(
            "必須の外部連携が欠けています。通知・書類の保存・メール送信のどれかが動かない可能性があります。\n{}",
            problems.iter().map(|r| format!("・{}", plain_problem(r))).collect::<Vec<_>>().join("\n")
        ),
        "設定 > 外部連携 で、欠けている連携を設定してください",
    );
    let ops_usable = !problems.iter().any(|r| r.decl.id == "ops_alert");
    if ops_usable {
        super::notify_ops(incident.to_notice()).await;
    } else {
        // 異常の通知そのものが使えない → 代わりの手段（管理者へのメール）で知らせる
        notify_admin_by_mail(&pool, &incident).await;
    }
}

/// 画面の上部の赤い帯に出す、いまの外部連携の状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertState {
    pub at: chrono::DateTime<chrono::Utc>,
    pub problems: Vec<String>,
}

/// 点検の結果(起動時・手動・画面の「点検する」の控え)と、毎朝の点検の記録のうち、新しいほうを選ぶ。
/// 同じ時刻なら、毎朝の点検(送信の失敗も見ている)を優先する。どちらも無ければ None(帯は出さない)。
pub fn latest_alert(
    snapshot: Option<(chrono::DateTime<chrono::Utc>, Vec<String>)>,
    daily: Option<(chrono::DateTime<chrono::Utc>, Vec<String>)>,
) -> Option<AlertState> {
    let pick = match (snapshot, daily) {
        (Some(s), Some(d)) => if s.0 > d.0 { s } else { d },
        (Some(s), None) => s,
        (None, Some(d)) => d,
        (None, None) => return None,
    };
    Some(AlertState { at: pick.0, problems: pick.1 })
}

/// 赤い帯の元になる、いまの状態を読む
pub async fn current_alert(pool: &PgPool) -> Option<AlertState> {
    let snapshot = super::get_snapshot(OVERALL_KEY).map(|s| (s.at, s.detail));
    let daily = match integration_repo::last_daily_check(pool).await {
        Ok(Some(d)) => {
            let problems = d.problems.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
            Some((d.run_at, problems))
        }
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("[外部連携] 毎朝の点検の記録を読めません: {e}");
            None
        }
    };
    latest_alert(snapshot, daily)
}

/// 画面の帯・通知に出す、人が読む1行(用途の ID や「必須」の記号を出さない。ログと CLI は従来の `summary_ja`)
fn plain_problem(r: &UseReport) -> String {
    r.plain_ja().unwrap_or_else(|| r.summary_ja())
}

/// 毎朝の点検で、前回と比べてどうするか
#[derive(Debug, PartialEq, Eq)]
pub enum DailyAction {
    /// 問題が新しく出た・内容が変わった → 知らせる
    Alert,
    /// 前回あった問題が、すべて無くなった → 復旧を知らせる
    Recovered,
    /// 前回と同じ(問題なしのまま、または同じ問題のまま) → 知らせない(毎日同じ警告を繰り返さない)
    Quiet,
}

/// 問題の一覧を、並べ替えて連結したもの(前回との比較用。空なら問題なし)
pub fn daily_signature(problems: &[String]) -> String {
    let mut v: Vec<&str> = problems.iter().map(String::as_str).collect();
    v.sort_unstable();
    v.join("\n")
}

/// 前回の signature(初回は None)と今回の問題から、知らせるかを決める
pub fn decide_daily(prev_signature: Option<&str>, problems: &[String]) -> DailyAction {
    let now = daily_signature(problems);
    let prev = prev_signature.unwrap_or("");
    match (prev.is_empty(), now.is_empty()) {
        (_, true) if !prev.is_empty() => DailyAction::Recovered,
        (_, true) => DailyAction::Quiet,
        _ if prev == now => DailyAction::Quiet,
        _ => DailyAction::Alert,
    }
}

/// 毎朝 8:45(JST)の点検。全部の連携を点検し、直近の実際の送信の失敗も見て、前回から変わったときだけ
/// 異常の通知(Chat)と管理者へのメールの**両方**で知らせる(通知の経路そのものが壊れていても気づけるように)。
/// 結果は毎回 h_integration_daily_check に残す。
pub async fn run_daily(pool: PgPool) {
    super::reload(&pool).await;
    let reports = run(&pool, true).await;
    let mut problems: Vec<String> = reports.iter().filter(|r| r.is_problem()).map(plain_problem).collect();

    // 直近の実際の送信が失敗している送り先(点検が通っても、届いていないことがある)
    match integration_repo::list_views(&pool).await {
        Ok(views) => {
            let jst = chrono::FixedOffset::east_opt(9 * 3600);
            for v in views.iter().filter(|v| v.use_id == super::NOTIFY_USE && v.enabled && v.last_delivery_ok == Some(false)) {
                let when = match (v.last_delivery_at, jst) {
                    (Some(t), Some(tz)) => t.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string(),
                    _ => "日時不明".to_string(),
                };
                problems.push(format!("異常の通知の送り先「{}」に、最後の送信が届いていません（{when}）", v.label));
            }
        }
        Err(e) => tracing::warn!("[外部連携の毎朝の点検] 送り先の送信結果を読めません: {e}"),
    }

    // 発行した PDF の Drive への保存が、24時間以上できていない(15分ごとのやり直しでも直らない)。
    // 途中の失敗は Chat に出さない(人が動くのは、続いているとき)。毎朝ここで見る
    match issued_document_repo::pending_older_than_hours(&pool, 24).await {
        Ok(n) if n > 0 => problems.push(format!(
            "発行した書類の PDF が、{n}件、24時間以上 Drive に保存できていません（Sophia の DB には残っています）"
        )),
        Ok(_) => {}
        Err(e) => tracing::warn!("[外部連携の毎朝の点検] 保存待ちの書類を数えられません: {e}"),
    }

    let prev = match integration_repo::last_daily_check(&pool).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[外部連携の毎朝の点検] 前回の結果を読めません。比べずに知らせます: {e}");
            None
        }
    };
    let action = decide_daily(prev.as_ref().map(|p| p.signature.as_str()), &problems);
    let signature = daily_signature(&problems);

    let list = problems.iter().map(|p| format!("・{p}")).collect::<Vec<_>>().join("\n");
    let (notified, detail) = match action {
        DailyAction::Quiet => {
            if problems.is_empty() {
                tracing::info!("[外部連携の毎朝の点検] すべて使えます");
            } else {
                tracing::warn!("[外部連携の毎朝の点検] 前回と同じ問題が続いています（知らせは前回済み）: {} 件", problems.len());
            }
            (false, "前回と同じ".to_string())
        }
        DailyAction::Alert => {
            let incident = Incident::new(
                Level::High,
                format!("毎朝の点検で、使えない外部連携・届いていない通知が見つかりました（{}件）", problems.len()),
                format!("必須の外部連携が欠けているか、通知が届いていません。業務への影響が出る前に直す必要があります。\n{list}"),
                "設定 > 外部連携 を開いて、赤い帯の項目を確認してください",
            );
            tracing::error!("[外部連携の毎朝の点検] {}", incident.plain_text().replace('\n', " / "));
            super::notify_ops(incident.to_notice()).await;
            notify_admin_by_mail(&pool, &incident).await;
            (true, "異常を知らせた".to_string())
        }
        DailyAction::Recovered => {
            let incident = Incident::new(
                Level::Recovered,
                "毎朝の点検で、前回見つかった外部連携の問題は、すべて解消していました",
                "なし（通知・書類の保存・メール送信は使える状態です）",
                "対応は不要です",
            );
            tracing::info!("[外部連携の毎朝の点検] {}", incident.summary);
            super::notify_ops(incident.to_notice()).await;
            notify_admin_by_mail(&pool, &incident).await;
            (true, "復旧を知らせた".to_string())
        }
    };
    if let Err(e) = integration_repo::insert_daily_check(&pool, &problems, &signature, notified, &detail).await {
        tracing::error!("[外部連携の毎朝の点検] 結果を記録できません: {e}");
    }
}

/// ops_alert の代わり（fallback = "mail_admin"）。本番・ステージング・DR のときだけ送る（手元の開発では送らない）
async fn notify_admin_by_mail(pool: &PgPool, incident: &Incident) {
    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "local".to_string());
    if !matches!(env_name.as_str(), "production" | "staging" | "dr") {
        tracing::warn!("[外部連携の点検] 異常の通知が使えません。ENV_NAME={env_name} のため、管理者へのメールは送りません");
        return;
    }
    let to = crate::domain::services::email_service::get_notify_email(pool).await;
    if to.trim().is_empty() {
        tracing::error!("[外部連携の点検] 異常の通知も、管理者へのメールの宛先も設定されていないため、知らせられません");
        return;
    }
    let subject = incident.mail_subject();
    let svc = crate::domain::services::email_service::EmailService::new(pool.clone());
    match svc.send(&to, None, &subject, &incident.plain_text()).await {
        Ok(()) => tracing::info!("[外部連携の点検] 管理者へメールで知らせました"),
        Err(e) => tracing::error!("[外部連携の点検] 管理者へのメールも送れませんでした: {e}"),
    }
}

/// `sophia check-integrations` の表示。問題があれば true
pub fn print_report(reports: &[UseReport]) -> bool {
    println!("外部連携の点検（integrations.toml）");
    for r in reports {
        println!("  {}", r.summary_ja());
    }
    let problems = reports.iter().filter(|r| r.is_problem()).count();
    // デプロイ前の点検(scripts/lib/deploy_common.sh)が読む行。書式を変えるときはそちらも直す
    let ids: Vec<&str> = reports.iter().filter(|r| r.is_problem()).map(|r| r.decl.id.as_str()).collect();
    println!("PROBLEM_USES={}", ids.join(","));
    if problems == 0 {
        println!("結果: 必須の連携はすべて使えます");
    } else {
        println!("結果: 必須の連携のうち {problems} 件が使えません");
    }
    problems > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_alert_prefers_the_newer_result() {
        let t = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&chrono::Utc);
        let old = (t("2026-10-12T00:00:00Z"), p(&["古い"]));
        let new = (t("2026-10-12T05:00:00Z"), p(&[]));
        // 手動の点検のほうが新しければ、そちら(直したあとに帯が残らない)
        assert_eq!(latest_alert(Some(new.clone()), Some(old.clone())).unwrap().problems, Vec::<String>::new());
        // 毎朝のほうが新しければ、そちら
        assert_eq!(latest_alert(Some(old.clone()), Some(new.clone())).unwrap().problems, Vec::<String>::new());
        // 同じ時刻なら毎朝を優先
        let same_daily = (old.0, p(&["毎朝"]));
        assert_eq!(latest_alert(Some(old.clone()), Some(same_daily)).unwrap().problems, p(&["毎朝"]));
        // 片方だけ・どちらも無い
        assert_eq!(latest_alert(None, Some(old.clone())).unwrap().problems, p(&["古い"]));
        assert_eq!(latest_alert(Some(old), None).unwrap().problems, p(&["古い"]));
        assert!(latest_alert(None, None).is_none());
    }

    fn p(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn daily_alerts_only_when_problems_change() {
        // 初回に問題があれば知らせる。問題なしなら静か
        assert_eq!(decide_daily(None, &p(&["a"])), DailyAction::Alert);
        assert_eq!(decide_daily(None, &[]), DailyAction::Quiet);
        // 同じ問題が続くあいだは繰り返さない(順番が違っても同じ)
        assert_eq!(decide_daily(Some(&daily_signature(&p(&["a", "b"]))), &p(&["b", "a"])), DailyAction::Quiet);
        // 問題が増えた・変わったら知らせる
        assert_eq!(decide_daily(Some("a"), &p(&["a", "b"])), DailyAction::Alert);
        assert_eq!(decide_daily(Some("a"), &p(&["c"])), DailyAction::Alert);
        // すべて解消したら復旧を知らせ、その後は静か
        assert_eq!(decide_daily(Some("a"), &[]), DailyAction::Recovered);
        assert_eq!(decide_daily(Some(""), &[]), DailyAction::Quiet);
    }

    #[test]
    fn failed_report_lists_reasons() {
        let r = CheckReport::new().pass("a").fail("b", "理由");
        assert_eq!(from_report(r), UseStatus::Failed(vec!["b: 理由".to_string()]));
        assert_eq!(from_report(CheckReport::new().pass("a")), UseStatus::Ok);
    }
}
