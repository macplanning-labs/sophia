/// domain/services/token_access.rs — 外部向けトークンリンク(/token/{uuid})の利用可否の判定
///
/// 認証なしのリンクで「見る・承諾する・アップロードする」ことを許してよいかを、1か所で決める。
/// 画面(HTML)・JSON API・PDF のどれも、ここの判定を使う(入口ごとに判定がずれて、
/// 画面は期限切れなのにAPIは通る、という状態にしないため。DEMO-000167 項目3)。
///
/// ## 規則
/// - 注文書: 「送付済(SENT)」の間だけ期限がある。承諾した後は、期限なしで見られる(稼働報告の毎月の提出に使うため)。
/// - 支払通知書: 未承諾の間だけ期限がある。起点は送信日時(mail_sent_at)。送信前(None)は期限なし扱い。
/// - 請求書: 未承諾の間だけ、送信日時(sent_at)から期限がある。承諾した後は、期限なしで見られる(DEMO-000223)。
/// - 稼働報告のアップロード: 承諾済みの注文書だけ。送付済(未承諾)・起票は受け付けない。

use chrono::{DateTime, Utc};

/// リンクを使わせない理由
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenDenial {
    /// 有効期限が切れている
    Expired,
    /// 注文書がまだ承諾されていない(稼働報告のアップロード不可)
    NotAccepted,
}

/// 発行(送信)から `expiry_days` 日を超えたか。起点が無い(未送信)なら期限なし。
pub fn is_expired(issued_at: Option<DateTime<Utc>>, now: DateTime<Utc>, expiry_days: i32) -> bool {
    match issued_at {
        Some(issued) => now.signed_duration_since(issued).num_days() > expiry_days as i64,
        None => false,
    }
}

/// 注文書: 承諾(受諾)してよいか。SENT 以外はここでは扱わない(呼び出し側が「すでに承諾済み」等を判断する)。
pub fn check_order_accept(
    status: &str,
    token_issued_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    expiry_days: i32,
) -> Result<(), TokenDenial> {
    if status == "SENT" && is_expired(token_issued_at, now, expiry_days) {
        return Err(TokenDenial::Expired);
    }
    Ok(())
}

/// 支払通知書: 見る・PDF・承諾してよいか。承諾済みなら期限なし。
pub fn check_notice_access(
    partner_accepted_at: Option<DateTime<Utc>>,
    mail_sent_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    expiry_days: i32,
) -> Result<(), TokenDenial> {
    if partner_accepted_at.is_none() && is_expired(mail_sent_at, now, expiry_days) {
        return Err(TokenDenial::Expired);
    }
    Ok(())
}

/// 請求書: 見る・PDF・承諾してよいか。未承諾の間だけ、送信日時からの期限がある。
/// **承諾したあとは、期限なしで見られる**(承諾した書類は、ずっと取り出せる。DEMO-000223。佐藤さんの方針 2026-10-13)。
pub fn check_invoice_access(
    client_accepted_at: Option<DateTime<Utc>>,
    sent_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    expiry_days: i32,
) -> Result<(), TokenDenial> {
    if client_accepted_at.is_none() && is_expired(sent_at, now, expiry_days) {
        return Err(TokenDenial::Expired);
    }
    Ok(())
}

/// 稼働報告のアップロードを受けてよいか。承諾済み(ACCEPTED 以降)の注文書だけ。
/// 起票(DRAFT)・送付済(SENT)は不可。
pub fn check_timesheet_upload(status: &str) -> Result<(), TokenDenial> {
    match status {
        "ACCEPTED" | "REPORT_RECEIVED" | "NOTICE_CREATED" | "NOTICE_CONFIRMED" | "PAID" => Ok(()),
        _ => Err(TokenDenial::NotAccepted),
    }
}

impl TokenDenial {
    /// 取引先に見せる文言
    pub fn message(&self) -> &'static str {
        match self {
            Self::Expired => "このリンクの有効期限が切れています。お手数ですが発注元までお問い合わせください。",
            Self::NotAccepted => "この注文書はまだ承諾されていないため、稼働報告を受け付けられません。先に注文書の承諾をお願いします。",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn 注文書_有効な送付済みは承諾できる() {
        assert_eq!(check_order_accept("SENT", Some(now() - Duration::days(3)), now(), 14), Ok(()));
    }

    #[test]
    fn 注文書_期限切れの送付済みは承諾できない() {
        assert_eq!(
            check_order_accept("SENT", Some(now() - Duration::days(15)), now(), 14),
            Err(TokenDenial::Expired)
        );
    }

    #[test]
    fn 注文書_ちょうど期限日数は通り_超えると止まる() {
        assert_eq!(check_order_accept("SENT", Some(now() - Duration::days(14)), now(), 14), Ok(()));
        assert!(check_order_accept("SENT", Some(now() - Duration::days(15)), now(), 14).is_err());
    }

    #[test]
    fn 注文書_発行日時が無ければ期限なし() {
        assert_eq!(check_order_accept("SENT", None, now(), 14), Ok(()));
    }

    #[test]
    fn 注文書_承諾済みは期限が過ぎても期限判定しない() {
        // 承諾済みの注文書は、表示と同じく期限を見ない(二重承諾は更新0件で何も起きない)
        assert_eq!(check_order_accept("ACCEPTED", Some(now() - Duration::days(400)), now(), 14), Ok(()));
    }

    #[test]
    fn 支払通知書_未承諾で期限内は通る() {
        assert_eq!(check_notice_access(None, Some(now() - Duration::days(2)), now(), 14), Ok(()));
    }

    #[test]
    fn 支払通知書_未承諾で期限切れは止まる() {
        assert_eq!(
            check_notice_access(None, Some(now() - Duration::days(30)), now(), 14),
            Err(TokenDenial::Expired)
        );
    }

    #[test]
    fn 支払通知書_承諾済みは期限が過ぎても見られる() {
        let accepted = Some(now() - Duration::days(40));
        assert_eq!(check_notice_access(accepted, Some(now() - Duration::days(60)), now(), 14), Ok(()));
    }

    #[test]
    fn 支払通知書_未送信は期限なし() {
        assert_eq!(check_notice_access(None, None, now(), 14), Ok(()));
    }

    #[test]
    fn 請求書_未承諾で期限内は通り_期限切れは止まる() {
        assert_eq!(check_invoice_access(None, Some(now() - Duration::days(1)), now(), 14), Ok(()));
        assert_eq!(
            check_invoice_access(None, Some(now() - Duration::days(20)), now(), 14),
            Err(TokenDenial::Expired)
        );
    }

    #[test]
    fn 請求書_承諾済みは期限が過ぎても見られる() {
        // 承諾した書類は、ずっと取り出せる(DEMO-000223)
        let accepted = Some(now() - Duration::days(30));
        assert_eq!(check_invoice_access(accepted, Some(now() - Duration::days(400)), now(), 14), Ok(()));
    }

    #[test]
    fn 請求書_送信前は期限なし() {
        assert_eq!(check_invoice_access(None, None, now(), 14), Ok(()));
    }

    #[test]
    fn アップロード_承諾済み以降だけ通る() {
        for s in ["ACCEPTED", "REPORT_RECEIVED", "NOTICE_CREATED", "NOTICE_CONFIRMED", "PAID"] {
            assert_eq!(check_timesheet_upload(s), Ok(()), "{s}");
        }
    }

    #[test]
    fn アップロード_未承諾と未知の状態は止まる() {
        for s in ["DRAFT", "SENT", "", "UNKNOWN"] {
            assert_eq!(check_timesheet_upload(s), Err(TokenDenial::NotAccepted), "{s}");
        }
    }
}
