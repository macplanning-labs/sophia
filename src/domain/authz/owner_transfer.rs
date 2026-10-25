//! domain/authz/owner_transfer.rs — 担当の引き継ぎ(付け替え)の入力検証(DEMO-000173 PR 6b。設計書 D15・U11)。
//!
//! DB に触れない純粋な関数。管理 API と、社員・ユーザーの無効化が、保存の前に呼ぶ。

use super::stamp::OwnerStamp;

/// 付け替えの起点(この行と、その配下をまとめて付け替える)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferRoot {
    /// 顧客(配下の案件・契約・発注・受注・請求書などを含む)。
    Client(i64),
    /// 案件(配下の契約・発注・受注・請求書などを含む)。
    Project(String),
}

/// 入力の誤り。
#[derive(Debug, PartialEq, Eq)]
pub enum TransferInputError {
    /// 顧客も案件も指定されていない。
    NoRoot,
    /// 顧客と案件の両方が指定されている。
    BothRoots,
    /// 案件IDが空。
    EmptyProjectId,
    /// 引き継ぎ先の組織が無い。
    MissingOrg,
    /// 引き継ぎ先の担当者が無い。
    MissingEmployee,
    /// 引き継ぎ元と同じ担当者を指している。
    SameEmployee,
}

/// 顧客ID・案件IDのどちらか一方だけを受け取り、起点にする。
pub fn parse_root(client_id: Option<i64>, project_id: Option<&str>) -> Result<TransferRoot, TransferInputError> {
    let project = project_id.map(str::trim);
    match (client_id, project) {
        (Some(_), Some(_)) => Err(TransferInputError::BothRoots),
        (Some(id), None) => Ok(TransferRoot::Client(id)),
        (None, Some("")) => Err(TransferInputError::EmptyProjectId),
        (None, Some(p)) => Ok(TransferRoot::Project(p.to_string())),
        (None, None) => Err(TransferInputError::NoRoot),
    }
}

/// 引き継ぎ先(組織と担当者の両方が必須)をスタンプにする。
pub fn parse_to(org_unit_id: Option<i64>, employee_id: Option<i64>) -> Result<OwnerStamp, TransferInputError> {
    match (org_unit_id, employee_id) {
        (Some(o), Some(e)) => Ok(OwnerStamp::new(o, e)),
        (None, _) => Err(TransferInputError::MissingOrg),
        (Some(_), None) => Err(TransferInputError::MissingEmployee),
    }
}

/// 引き継ぎ元と同じ担当者への引き継ぎを拒否する(社員の無効化・担当者単位の引き継ぎ)。
pub fn check_not_same_employee(from_employee_id: i64, to: &OwnerStamp) -> Result<(), TransferInputError> {
    if to.employee_id == Some(from_employee_id) {
        return Err(TransferInputError::SameEmployee);
    }
    Ok(())
}

/// 利用者向けのメッセージ。
pub fn transfer_input_error_message(e: &TransferInputError) -> String {
    match e {
        TransferInputError::NoRoot => "顧客(client_id)か案件(project_id)のどちらかを指定してください".to_string(),
        TransferInputError::BothRoots => "顧客(client_id)と案件(project_id)は、どちらか一方だけ指定してください".to_string(),
        TransferInputError::EmptyProjectId => "案件IDを入力してください".to_string(),
        TransferInputError::MissingOrg => "引き継ぎ先の組織を選んでください".to_string(),
        TransferInputError::MissingEmployee => "引き継ぎ先の担当者を選んでください".to_string(),
        TransferInputError::SameEmployee => "引き継ぎ先に、引き継ぎ元と同じ担当者は選べません".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_needs_exactly_one_of_client_or_project() {
        assert_eq!(parse_root(Some(1), None), Ok(TransferRoot::Client(1)));
        assert_eq!(parse_root(None, Some(" PRJ0001 ")), Ok(TransferRoot::Project("PRJ0001".to_string())));
        assert_eq!(parse_root(None, None), Err(TransferInputError::NoRoot));
        assert_eq!(parse_root(Some(1), Some("PRJ0001")), Err(TransferInputError::BothRoots));
        assert_eq!(parse_root(None, Some("  ")), Err(TransferInputError::EmptyProjectId));
    }

    #[test]
    fn destination_must_be_complete() {
        assert_eq!(parse_to(Some(1), Some(2)), Ok(OwnerStamp::new(1, 2)));
        assert_eq!(parse_to(None, Some(2)), Err(TransferInputError::MissingOrg));
        assert_eq!(parse_to(None, None), Err(TransferInputError::MissingOrg));
        assert_eq!(parse_to(Some(1), None), Err(TransferInputError::MissingEmployee));
    }

    #[test]
    fn same_employee_is_rejected() {
        assert_eq!(check_not_same_employee(5, &OwnerStamp::new(1, 5)), Err(TransferInputError::SameEmployee));
        assert_eq!(check_not_same_employee(5, &OwnerStamp::new(1, 6)), Ok(()));
    }
}
