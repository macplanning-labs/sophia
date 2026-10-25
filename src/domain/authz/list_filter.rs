//! domain/authz/list_filter.rs — 一覧の絞り込み条件(設計書 §7.5)。
//!
//! 判定関数 `Authz::can` と同じ意味を、一覧取得(SQL)用の「条件の和」として表す。
//! ここでは SQL を作らず、メモリ上の評価 `matches` で意味を固定する(SQL 側はこれと同じ結果になること)。

use super::decision::Authz;
use super::permission::Permission;
use super::scope::DataScope;
use super::stamp::OwnerStamp;

/// 絞り込み条件の1項。ListFilter の中ではどれか1つでも当てはまれば見える(和集合。D8)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clause {
    /// すべて見える。
    All,
    /// 担当組織が `orgs` に含まれる。`employee` があれば担当者もそれと一致すること。
    OrgIn { orgs: Vec<i64>, employee: Option<i64> },
    /// 担当組織が未設定のもの(移行期間のみ)。
    Unassigned,
}

/// 一覧の絞り込み条件。空なら何も見えない。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListFilter(pub Vec<Clause>);

impl ListFilter {
    /// 何も見えない条件か。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// すべて見える条件を含むか。
    pub fn allows_all(&self) -> bool {
        self.0.iter().any(|c| matches!(c, Clause::All))
    }

    /// スタンプがこの条件で見えるか(SQL と同じ意味のメモリ評価)。
    pub fn matches(&self, stamp: &OwnerStamp) -> bool {
        self.0.iter().any(|c| match c {
            Clause::All => true,
            Clause::OrgIn { orgs, employee } => {
                stamp.org_unit_id.is_some_and(|o| orgs.contains(&o)) && employee.is_none_or(|e| stamp.employee_id == Some(e))
            }
            Clause::Unassigned => stamp.org_unit_id.is_none(),
        })
    }
}

impl Authz {
    /// 権限 `perm` で見える範囲を、一覧用の条件にする。`can` と同じ結果になること。
    pub fn list_filter(&self, perm: Permission) -> ListFilter {
        if self.is_staff() {
            return ListFilter(vec![Clause::All]);
        }
        if !perm.is_stamped() {
            // スタンプの無いデータは、権限があるかどうかだけで決まる。
            return if self.has_any(perm) { ListFilter(vec![Clause::All]) } else { ListFilter::default() };
        }
        let mut out = Vec::new();
        // 割り当て1行ごとに条件を作る(行をまたいで混ぜない。P4)。
        for g in self.grants_of(perm) {
            match g.scope {
                DataScope::Global => out.push(Clause::All),
                DataScope::Hierarchical => out.push(Clause::OrgIn { orgs: self.tree().subtree_ids(g.org_unit_id), employee: None }),
                DataScope::Unit => out.push(Clause::OrgIn { orgs: vec![g.org_unit_id], employee: None }),
                DataScope::Private => {
                    // 本人が特定できなければ、本人限定は何も許さない。
                    if let Some(me) = self.employee_id() {
                        out.push(Clause::OrgIn { orgs: self.tree().subtree_ids(g.org_unit_id), employee: Some(me) });
                    }
                }
            }
        }
        if self.unassigned_applies(perm) {
            out.push(Clause::Unassigned);
        }
        ListFilter(out)
    }
}
