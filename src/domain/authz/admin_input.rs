//! domain/authz/admin_input.rs — 組織・ロール管理画面の入力検証と、エラーの日本語メッセージ(DEMO-000173 PR 5)。
//!
//! DB に触れない純粋な関数。管理 API のハンドラが、保存の前に呼ぶ。

use super::decision::AssignmentError;
use super::org_tree::{OrgTreeError, MAX_ORG_DEPTH};

/// 組織名・ロール名の最大文字数(DB の VARCHAR(100) に合わせる)。
pub const MAX_NAME_CHARS: usize = 100;
/// 組織の種別・ロールコードの最大文字数(DB の VARCHAR(50) に合わせる)。
pub const MAX_CODE_CHARS: usize = 50;

/// 名前を整える(前後の空白を除く)。空・長すぎる場合はエラーメッセージを返す。
pub fn normalize_name(raw: &str, what: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(format!("{what}を入力してください"));
    }
    if s.chars().count() > MAX_NAME_CHARS {
        return Err(format!("{what}は{MAX_NAME_CHARS}文字以内で入力してください"));
    }
    Ok(s.to_string())
}

/// 組織の種別(任意)を整える。空は `None`。
pub fn normalize_unit_kind(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if s.chars().count() > MAX_CODE_CHARS {
        return Err(format!("組織の種別は{MAX_CODE_CHARS}文字以内で入力してください"));
    }
    Ok(Some(s.to_string()))
}

/// ロールコードを検証する。英小文字で始まり、英小文字・数字・'_' のみ(50文字以内)。
pub fn validate_role_code(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok = s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !first_ok || !rest_ok || s.chars().count() > MAX_CODE_CHARS {
        return Err(format!("ロールコードは、英小文字で始まる英小文字・数字・アンダースコアの{MAX_CODE_CHARS}文字以内で入力してください"));
    }
    Ok(s.to_string())
}

/// 一覧取得の件数を整える(既定100、上限500、0以下は既定)。
pub fn clamp_limit(limit: Option<i64>) -> i64 {
    match limit {
        Some(n) if n > 0 => n.min(500),
        _ => 100,
    }
}

/// 組織の木の操作エラーを、画面に出す日本語にする。
pub fn org_tree_error_message(e: &OrgTreeError) -> String {
    match e {
        OrgTreeError::WouldCreateCycle => "ルート組織は動かせません。また、自分自身や自分の配下の下には付け替えられません".to_string(),
        OrgTreeError::TooDeep(_) => format!("組織の階層は最大{MAX_ORG_DEPTH}段までです"),
        OrgTreeError::UnknownNode(_) | OrgTreeError::UnknownParent(_) => "指定した組織が見つかりません".to_string(),
        OrgTreeError::NoRoot | OrgTreeError::MultipleRoots | OrgTreeError::Cycle(_) | OrgTreeError::DuplicateId(_) => {
            "組織データに不整合があります。管理者に連絡してください".to_string()
        }
    }
}

/// 割り当て・権限セットの検証エラーを、画面に出す日本語にする。
pub fn assignment_error_message(e: &AssignmentError) -> String {
    match e {
        AssignmentError::UnknownOrg => "指定した組織が見つかりません".to_string(),
        AssignmentError::GlobalStampedOutsideRoot => {
            "契約・注文・請求・勤務表・取引先・案件の権限に「全社」を持つロールは、ルート組織にだけ割り当てられます".to_string()
        }
        AssignmentError::UnknownCodeOrScope(code) => format!("不明な権限コードまたは範囲です: {code}"),
        AssignmentError::NonGlobalScopeOnUnstamped(code) => format!("この権限は範囲を分けられません(全社のみ): {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_trimmed_and_bounded() {
        assert_eq!(normalize_name("  営業部 ", "組織名").unwrap(), "営業部");
        assert!(normalize_name("   ", "組織名").is_err());
        assert!(normalize_name(&"あ".repeat(100), "組織名").is_ok());
        assert!(normalize_name(&"あ".repeat(101), "組織名").is_err());
    }

    #[test]
    fn unit_kind_is_optional() {
        assert_eq!(normalize_unit_kind(None).unwrap(), None);
        assert_eq!(normalize_unit_kind(Some("  ")).unwrap(), None);
        assert_eq!(normalize_unit_kind(Some(" 課 ")).unwrap(), Some("課".to_string()));
        assert!(normalize_unit_kind(Some(&"x".repeat(51))).is_err());
    }

    #[test]
    fn role_code_format() {
        assert_eq!(validate_role_code("sales_2").unwrap(), "sales_2");
        for bad in ["", "Sales", "2sales", "sa-les", "営業", &"a".repeat(51)] {
            assert!(validate_role_code(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn limit_defaults_and_caps() {
        assert_eq!(clamp_limit(None), 100);
        assert_eq!(clamp_limit(Some(0)), 100);
        assert_eq!(clamp_limit(Some(-3)), 100);
        assert_eq!(clamp_limit(Some(20)), 20);
        assert_eq!(clamp_limit(Some(9999)), 500);
    }

    #[test]
    fn error_messages_are_japanese_and_non_empty() {
        assert!(org_tree_error_message(&OrgTreeError::WouldCreateCycle).contains("ルート"));
        assert!(org_tree_error_message(&OrgTreeError::TooDeep(1)).contains('8'));
        assert!(!assignment_error_message(&AssignmentError::GlobalStampedOutsideRoot).is_empty());
        assert!(assignment_error_message(&AssignmentError::UnknownCodeOrScope("x:y".into())).contains("x:y"));
    }
}
