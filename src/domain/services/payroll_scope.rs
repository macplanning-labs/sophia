//! 給与一覧の閲覧範囲の判定
//!
//! 給与データの閲覧範囲を、呼び出し者の権限と社員紐付けだけから決める純粋関数。
//! - 全件権限(`can_view_all_payroll`)があれば全件
//! - 無ければ、社員に紐づいていれば本人分のみ
//! - 権限も社員紐付けも無ければ、対象なし(0件)

/// 給与データの閲覧範囲
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayrollScope {
    /// 全件
    All,
    /// 指定した社員の分のみ
    Own(i64),
    /// 対象なし
    Nothing,
}

/// 呼び出し者の権限と社員紐付けから閲覧範囲を決める。
pub fn payroll_scope(can_view_all: bool, employee_id: Option<i64>) -> PayrollScope {
    if can_view_all {
        PayrollScope::All
    } else {
        match employee_id {
            Some(e) => PayrollScope::Own(e),
            None => PayrollScope::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 全件権限があれば社員紐付けの有無に関わらず全件() {
        assert_eq!(payroll_scope(true, Some(7)), PayrollScope::All);
    }

    #[test]
    fn 全件権限がありプロフィールなしでも全件() {
        assert_eq!(payroll_scope(true, None), PayrollScope::All);
    }

    #[test]
    fn 全件権限がなく社員紐付けがあれば本人分のみ() {
        assert_eq!(payroll_scope(false, Some(7)), PayrollScope::Own(7));
    }

    #[test]
    fn 全件権限がなく社員紐付けもなければ対象なし() {
        assert_eq!(payroll_scope(false, None), PayrollScope::Nothing);
    }
}
