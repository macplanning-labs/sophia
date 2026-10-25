//! domain/authz/scope.rs — データセキュリティ・スコープ(設計書 §7.2)。
//!
//! ロールが権限ごとに持つ「どこまで見える・操作できるか」の4段階。DB(`m_role_permission.scope`)には大文字の文字列で持つ。

/// データセキュリティ・スコープ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataScope {
    /// 本人限定: データの担当者が自分で、かつ担当組織が割り当ての組織の配下にあるもの。
    Private,
    /// 自組織限定: データの担当組織が、割り当ての組織と完全に同じもの。
    Unit,
    /// 組織配下: データの担当組織が、割り当ての組織とその配下にあるもの。
    Hierarchical,
    /// 全社: 組織の枠を超えて、そのドメインの全データ。
    Global,
}

impl DataScope {
    /// 全スコープ(強い順ではなく、定義順)。
    pub const ALL: [DataScope; 4] = [DataScope::Private, DataScope::Unit, DataScope::Hierarchical, DataScope::Global];

    /// DB の文字列から引く。不明な値は `None`(拒否側に倒す)。
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "PRIVATE" => Some(DataScope::Private),
            "UNIT" => Some(DataScope::Unit),
            "HIERARCHICAL" => Some(DataScope::Hierarchical),
            "GLOBAL" => Some(DataScope::Global),
            _ => None,
        }
    }

    /// DB に保存する文字列。
    pub fn to_db(self) -> &'static str {
        match self {
            DataScope::Private => "PRIVATE",
            DataScope::Unit => "UNIT",
            DataScope::Hierarchical => "HIERARCHICAL",
            DataScope::Global => "GLOBAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DataScope;

    #[test]
    fn db_string_round_trips() {
        for s in DataScope::ALL {
            assert_eq!(DataScope::from_db(s.to_db()), Some(s));
        }
        assert_eq!(DataScope::from_db("global"), None);
        assert_eq!(DataScope::from_db(""), None);
    }
}
