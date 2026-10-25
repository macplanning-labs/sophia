/// domain/services/sent_document_lock.rs — 相手に送った書類は、削除・書き換えをしない(DEMO-000223)
///
/// 佐藤さんの方針(2026-10-13):
/// - 当社が発行して、相手に確認を依頼した書類は、削除してはいけない。内容を変えたいときは、別の書類(新しい版)を作って、
///   もう一度承認をもらう。そうしないと「何がダメだったのか」が分からなくなる。
/// - 相手が承認した書類は、絶対に削除しない。
///
/// 「相手に確認を依頼した」= メールで相手に送った時点(請求書は `sent_at`、支払通知書は `mail_sent_at`)。
/// 送る前の下書き・社内の承認の段階は、今までどおり削除・編集できる。承諾の記録がある書類も、送ったものとして扱う。
/// 画面・API で止めるほか、データベースの側(マイグレーション 079)でも同じ規則で拒む。
use chrono::{DateTime, Utc};

/// 請求書: 送った、または承諾された
pub fn invoice_is_locked(sent_at: Option<DateTime<Utc>>, client_accepted_at: Option<DateTime<Utc>>) -> bool {
    sent_at.is_some() || client_accepted_at.is_some()
}

/// 支払通知書: 送った、または承諾された
pub fn notice_is_locked(mail_sent_at: Option<DateTime<Utc>>, partner_accepted_at: Option<DateTime<Utc>>) -> bool {
    mail_sent_at.is_some() || partner_accepted_at.is_some()
}

/// 削除しようとして止めるときの文言(画面に出る)
pub fn delete_denied_message(doc_label: &str) -> String {
    format!("相手に送った{doc_label}は、削除できません(何がダメだったかが分からなくなるため)。内容を変えるときは、新しい版を作って、もう一度送ってください。")
}

/// 書き換えようとして止めるときの文言(画面に出る)
pub fn edit_denied_message(doc_label: &str) -> String {
    format!("相手に送った{doc_label}は、書き換えできません(送った内容が分からなくなるため)。内容を変えるときは、新しい版を作って、もう一度送ってください。")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Option<DateTime<Utc>> {
        Some(Utc::now())
    }

    #[test]
    fn 請求書_送る前は削除・編集できる() {
        assert!(!invoice_is_locked(None, None));
    }

    #[test]
    fn 請求書_送った・承諾したら固定される() {
        assert!(invoice_is_locked(t(), None));
        assert!(invoice_is_locked(t(), t()));
        assert!(invoice_is_locked(None, t()), "承諾の記録があれば、送ったものとして扱う");
    }

    #[test]
    fn 支払通知書_送る前は削除・編集できる() {
        assert!(!notice_is_locked(None, None));
    }

    #[test]
    fn 支払通知書_送った・承諾したら固定される() {
        assert!(notice_is_locked(t(), None));
        assert!(notice_is_locked(t(), t()));
        assert!(notice_is_locked(None, t()));
    }

    #[test]
    fn 文言は理由と次の手順を言う() {
        assert!(delete_denied_message("請求書").contains("削除できません"));
        assert!(edit_denied_message("支払通知書").contains("新しい版"));
    }
}
