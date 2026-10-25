/// domain/services/document_revision.rs — 送った書類の「訂正版」と「取り消し」(DEMO-000224)
///
/// 佐藤さんの方針(2026-10-13・10-19。docs/spec/要件定義書.md §3.11、アプリ方式設計書.md §8.3.1):
/// - 送った書類は、削除も書き換えもしない(DEMO-000223)。
/// - 内容を直したいときは「訂正版」を作る: 同じ内容の新しい版(下書き)ができ、前の版は「差し替え済み」として残る。
///   新しい版は、もう一度、承認 → 送信 → 相手の承諾を通す。変えた理由を残す。
/// - 書類自体をなかったことにするときは「取り消し」: 書類は消さず「取り消し済み」で残し、理由を残す。
/// - 承諾済みの版も同じ。差し替え済み・取り消し済みになっても残り、相手からずっと見える。
/// - 対象は請求書と支払通知書(注文書は対象外)。
///
/// 「差し替え済み」は列で持たず、「この書類を差し替えた版がある」ことで表す(送った書類の行を更新しないため)。
use crate::domain::services::sent_document_lock;
use chrono::{DateTime, Utc};

/// 書類1件の、版に関する状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionState {
    /// 送った、または承諾された(sent_document_lock と同じ判定)
    pub locked: bool,
    /// 取り消し済み
    pub cancelled: bool,
    /// 新しい版に差し替えられた(この書類を差し替えた版がある)
    pub superseded: bool,
}

impl RevisionState {
    pub fn of_invoice(
        sent_at: Option<DateTime<Utc>>,
        client_accepted_at: Option<DateTime<Utc>>,
        cancelled_at: Option<DateTime<Utc>>,
        superseded: bool,
    ) -> Self {
        Self {
            locked: sent_document_lock::invoice_is_locked(sent_at, client_accepted_at),
            cancelled: cancelled_at.is_some(),
            superseded,
        }
    }

    pub fn of_notice(
        mail_sent_at: Option<DateTime<Utc>>,
        partner_accepted_at: Option<DateTime<Utc>>,
        cancelled_at: Option<DateTime<Utc>>,
        superseded: bool,
    ) -> Self {
        Self {
            locked: sent_document_lock::notice_is_locked(mail_sent_at, partner_accepted_at),
            cancelled: cancelled_at.is_some(),
            superseded,
        }
    }

    /// 生きている版(取り消し済みでも、差し替え済みでもない)
    pub fn is_current(&self) -> bool {
        !self.cancelled && !self.superseded
    }

    /// 訂正版を作れる: 送った書類で、生きている版
    pub fn can_revise(&self) -> bool {
        self.locked && self.is_current()
    }

    /// 取り消せる: 送った書類で、生きている版
    pub fn can_cancel(&self) -> bool {
        self.locked && self.is_current()
    }

    /// 承認・差し戻し・送信などの操作ができる(取り消し済み・差し替え済みの書類には、させない)
    pub fn can_operate(&self) -> bool {
        self.is_current()
    }

    /// 画面に出す状態の名前(通常の状態は None)
    pub fn label(&self) -> Option<&'static str> {
        if self.cancelled {
            Some("取り消し済み")
        } else if self.superseded {
            Some("差し替え済み")
        } else {
            None
        }
    }
}

/// 訂正版を作る・取り消すの結果
#[derive(Debug, PartialEq, Eq)]
pub enum RevisionOutcome<T> {
    /// できた(訂正版の番号、または取り消した書類の番号)
    Done(T),
    NotFound,
    /// まだ相手に送っていない(送る前は、そのまま編集・削除できる)
    NotSent,
    /// 取り消し済み
    Cancelled,
    /// 新しい版に差し替え済み
    Superseded,
}

/// 訂正版の番号。元の番号の末尾に版の番号を付ける(PN-202610-001 → PN-202610-001-2 → PN-202610-001-3)
pub fn next_revision_number(current: &str, current_revision: i32) -> String {
    let root = if current_revision > 1 {
        current.strip_suffix(&format!("-{current_revision}")).unwrap_or(current)
    } else {
        current
    };
    format!("{root}-{}", current_revision + 1)
}

/// 理由の確認(前後の空白を除いた中身が必要。長すぎるものは断る)
pub fn validate_reason(reason: &str) -> Result<String, &'static str> {
    let r = reason.trim();
    if r.is_empty() {
        return Err("理由を入力してください");
    }
    if r.chars().count() > 500 {
        return Err("理由は500文字以内で入力してください");
    }
    Ok(r.to_string())
}

/// 訂正版・取り消しが断られたときの、状態コードと文言(できたときは None)
pub fn outcome_error<T>(doc_label: &str, outcome: &RevisionOutcome<T>) -> Option<(u16, String)> {
    match outcome {
        RevisionOutcome::Done(_) => None,
        RevisionOutcome::NotFound => Some((404, format!("{doc_label}が見つかりません"))),
        RevisionOutcome::NotSent => Some((409, not_sent_message(doc_label))),
        RevisionOutcome::Cancelled => Some((409, format!("取り消し済みの{doc_label}です"))),
        RevisionOutcome::Superseded => Some((
            409,
            format!("すでに新しい版に差し替え済みの{doc_label}です。新しい版で操作してください"),
        )),
    }
}

/// 操作を止めるときの文言(画面に出る)
pub fn not_current_message(doc_label: &str, state: &RevisionState) -> String {
    if state.cancelled {
        format!("取り消し済みの{doc_label}は、操作できません")
    } else {
        format!("新しい版に差し替え済みの{doc_label}は、操作できません。新しい版で操作してください")
    }
}

pub fn not_sent_message(doc_label: &str) -> String {
    format!("まだ相手に送っていない{doc_label}です。送る前は、そのまま編集・削除できます")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Option<DateTime<Utc>> {
        Some(Utc::now())
    }

    #[test]
    fn 送る前は訂正版も取り消しもできない() {
        let s = RevisionState::of_notice(None, None, None, false);
        assert!(!s.can_revise());
        assert!(!s.can_cancel());
        assert!(s.can_operate());
        assert_eq!(s.label(), None);
    }

    #[test]
    fn 送った書類は訂正版も取り消しもできる() {
        let s = RevisionState::of_invoice(t(), None, None, false);
        assert!(s.can_revise());
        assert!(s.can_cancel());
    }

    #[test]
    fn 承諾済みの書類も訂正版を作れる() {
        let s = RevisionState::of_notice(t(), t(), None, false);
        assert!(s.can_revise(), "承諾済みも、差し替え済みとして残しつつ、訂正版を作れる");
    }

    #[test]
    fn 差し替え済みの書類は_もう訂正版も取り消しもできず_操作もできない() {
        let s = RevisionState::of_invoice(t(), t(), None, true);
        assert!(!s.can_revise());
        assert!(!s.can_cancel());
        assert!(!s.can_operate());
        assert_eq!(s.label(), Some("差し替え済み"));
    }

    #[test]
    fn 取り消し済みの書類は_もう何もできない() {
        let s = RevisionState::of_notice(t(), None, t(), false);
        assert!(!s.can_revise());
        assert!(!s.can_cancel());
        assert!(!s.can_operate());
        assert_eq!(s.label(), Some("取り消し済み"));
    }

    #[test]
    fn 取り消しは差し替えより先に表示する() {
        let s = RevisionState::of_notice(t(), None, t(), true);
        assert_eq!(s.label(), Some("取り消し済み"));
    }

    #[test]
    fn 訂正版の番号は版ごとに末尾を付け替える() {
        assert_eq!(next_revision_number("PN-202610-001", 1), "PN-202610-001-2");
        assert_eq!(next_revision_number("PN-202610-001-2", 2), "PN-202610-001-3");
        assert_eq!(next_revision_number("INV-202609-001-9", 9), "INV-202609-001-10");
        assert_eq!(next_revision_number("2606001", 1), "2606001-2");
    }

    #[test]
    fn 番号の末尾が版と合わないときは_そのまま付ける() {
        assert_eq!(next_revision_number("PN-1", 3), "PN-1-4");
    }

    #[test]
    fn 理由は必須で_前後の空白は除く() {
        assert_eq!(validate_reason("  金額の訂正  "), Ok("金額の訂正".to_string()));
        assert!(validate_reason("   ").is_err());
        assert!(validate_reason("").is_err());
        assert!(validate_reason(&"あ".repeat(501)).is_err());
        assert!(validate_reason(&"あ".repeat(500)).is_ok());
    }

    #[test]
    fn 断られた結果は状態コードと文言になる() {
        assert_eq!(outcome_error("請求書", &RevisionOutcome::Done(1)), None);
        assert_eq!(outcome_error("請求書", &RevisionOutcome::<i32>::NotFound).unwrap().0, 404);
        let (code, msg) = outcome_error("請求書", &RevisionOutcome::<i32>::NotSent).unwrap();
        assert_eq!(code, 409);
        assert!(msg.contains("まだ相手に送っていない"));
        assert!(outcome_error("請求書", &RevisionOutcome::<i32>::Cancelled).unwrap().1.contains("取り消し済み"));
        assert!(outcome_error("請求書", &RevisionOutcome::<i32>::Superseded).unwrap().1.contains("新しい版"));
    }

    #[test]
    fn 止める文言は理由を言う() {
        let c = RevisionState::of_notice(t(), None, t(), false);
        let s = RevisionState::of_notice(t(), None, None, true);
        assert!(not_current_message("支払通知書", &c).contains("取り消し済み"));
        assert!(not_current_message("支払通知書", &s).contains("新しい版"));
        assert!(not_sent_message("請求書").contains("まだ相手に送っていない"));
    }
}
