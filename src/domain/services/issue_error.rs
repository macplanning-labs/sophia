//! 請求書・支払通知書の一括発行の失敗(DEMO-000173 PR 7a)。
//!
//! 従来は文言(`String`)だけだったが、「担当が決まらない」だけは呼び出し側(HTTP層)が 400 にして
//! 案内を返す必要があるため、文字列比較をせずに見分けられる型にする。

use crate::domain::authz::stamp::NO_OWNER_MESSAGE;

/// 一括発行の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueError {
    /// 顧客・案件に担当が無く、請求書・支払通知書の担当を決められない(HTTP 400)。全体を取り消している。
    NoOwner,
    /// それ以外の失敗(利用者向けの文言)。
    Message(String),
}

impl From<String> for IssueError {
    fn from(m: String) -> Self {
        IssueError::Message(m)
    }
}

impl std::fmt::Display for IssueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueError::NoOwner => f.write_str(NO_OWNER_MESSAGE),
            IssueError::Message(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for IssueError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_converts_to_message() {
        let e: IssueError = String::from("失敗").into();
        assert_eq!(e, IssueError::Message("失敗".into()));
        assert_eq!(e.to_string(), "失敗");
    }

    #[test]
    fn no_owner_shows_guidance() {
        assert_eq!(IssueError::NoOwner.to_string(), NO_OWNER_MESSAGE);
    }
}
