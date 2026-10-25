//! 経費(ヘッダー・明細・領収書)を操作してよいかの判定
//!
//! 一覧・詳細・更新・承認系と同じ考え方で、「全件権限者、または申請者本人」だけを通す。
//! 認可の仕組み(role.rs)は変更せず、ハンドラ内で最小限の確認をするための純粋関数。

/// 経費(の明細・領収書)に触ってよいか。
///
/// - `can_manage`: `AuthUser::can_see_all_expenses()`(管理者、経費の全件スイッチ、またはロールの `expense.claim:view`)
/// - `caller_employee_id`: 呼び出し者の社員ID(プロフィールが無ければ None)
/// - `owner_employee_id`: 経費の申請者の社員ID
///
/// プロフィールの無い呼び出し者は、`can_manage` でなければ拒否する(他人の経費と同じ扱い)。
pub fn can_access(can_manage: bool, caller_employee_id: Option<i64>, owner_employee_id: i64) -> bool {
    can_manage || caller_employee_id == Some(owner_employee_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 全件権限者は本人でなくても許可される() {
        assert!(can_access(true, Some(1), 2));
    }

    #[test]
    fn 全件権限者はプロフィールが無くても許可される() {
        assert!(can_access(true, None, 2));
    }

    #[test]
    fn 全件権限者が本人の経費でも許可される() {
        assert!(can_access(true, Some(1), 1));
    }

    #[test]
    fn 権限が無い本人は許可される() {
        assert!(can_access(false, Some(1), 1));
    }

    #[test]
    fn 権限が無く他人の経費は拒否される() {
        assert!(!can_access(false, Some(1), 2));
    }

    #[test]
    fn 権限が無くプロフィールが無い呼び出し者は拒否される() {
        assert!(!can_access(false, None, 2));
    }
}
