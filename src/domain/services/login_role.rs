//! ログイン時のロール判定(純関数)。パスワードログインと社内 SSO で同じ判定を使う(DEMO-000231)。
//!
//! 優先順位: 管理者(is_staff) > 社員(employee_id あり) > パートナー(partner_id あり) > その他(USER)。
//! プロフィールの取得(DB)は呼び出し側で行う。**DB エラーをどう扱うかは、呼び出し側が決める**
//! (パスワードログインは従来どおりログに残して USER 扱い、SSO は 500 で止める。方式設計書 §3.5.4)。

/// プロフィールのロール判定用の項目: (partner_id, employee_id)。
pub type ProfileRoleFields = Option<(Option<String>, Option<i64>)>;

/// ロール名を返す("ADMIN" | "EMPLOYEE" | "PARTNER" | "USER")。
pub fn role_for_login(is_staff: bool, profile: &ProfileRoleFields) -> &'static str {
    if is_staff {
        return "ADMIN";
    }
    match profile {
        Some((_, Some(_))) => "EMPLOYEE",
        Some((Some(_), _)) => "PARTNER",
        _ => "USER",
    }
}

/// 社内 SSO でログインしてよいロールか(社員・管理者だけ。パートナー・エンジニアなど社外の人は対象外。
/// DEMO-000226 決定 1)。
pub fn is_sso_eligible_role(role: &str) -> bool {
    matches!(role, "ADMIN" | "EMPLOYEE")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 管理者は_プロフィールに関わらず_admin() {
        assert_eq!(role_for_login(true, &None), "ADMIN");
        assert_eq!(role_for_login(true, &Some((Some("P1".into()), Some(3)))), "ADMIN");
    }

    #[test]
    fn 社員は_employee_id_があれば_partner_id_より優先() {
        assert_eq!(role_for_login(false, &Some((None, Some(3)))), "EMPLOYEE");
        assert_eq!(role_for_login(false, &Some((Some("P1".into()), Some(3)))), "EMPLOYEE");
    }

    #[test]
    fn パートナーは_partner_id_だけ() {
        assert_eq!(role_for_login(false, &Some((Some("P1".into()), None))), "PARTNER");
    }

    #[test]
    fn どちらも無ければ_user() {
        assert_eq!(role_for_login(false, &Some((None, None))), "USER");
        assert_eq!(role_for_login(false, &None), "USER");
    }

    #[test]
    fn sso_は_社員と管理者だけ() {
        assert!(is_sso_eligible_role("ADMIN"));
        assert!(is_sso_eligible_role("EMPLOYEE"));
        assert!(!is_sso_eligible_role("PARTNER"));
        assert!(!is_sso_eligible_role("USER"));
        assert!(!is_sso_eligible_role("ENGINEER"));
        assert!(!is_sso_eligible_role(""));
    }
}
