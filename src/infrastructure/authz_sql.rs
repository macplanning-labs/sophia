//! infrastructure/authz_sql.rs — 一覧の絞り込み条件(`ListFilter`)を、SQL の WHERE 句にする(設計書 §2.5)。
//!
//! 列名は呼び出し側のコードで固定し(ユーザー入力にしない)、値は必ずバインドする。
//! 意味は `ListFilter::matches`(メモリ上の評価)と同じ。空なら `FALSE`、`All` を含めば `TRUE`。

use crate::domain::authz::list_filter::{Clause, ListFilter};
use sqlx::{Postgres, QueryBuilder};

/// 条件を `qb` に書き足す。`WHERE` や `AND` は呼び出し側が付ける(`(...)` で囲むので、`AND` で連結してよい)。
pub fn push_where(qb: &mut QueryBuilder<'_, Postgres>, f: &ListFilter, org_col: &str, emp_col: &str) {
    if f.allows_all() {
        qb.push("TRUE");
        return;
    }
    if f.is_empty() {
        qb.push("FALSE");
        return;
    }
    qb.push("(");
    for (i, c) in f.0.iter().enumerate() {
        if i > 0 {
            qb.push(" OR ");
        }
        match c {
            Clause::All => {
                qb.push("TRUE");
            }
            Clause::OrgIn { orgs, employee } => {
                qb.push("(").push(org_col).push(" = ANY(").push_bind(orgs.clone()).push(")");
                if let Some(e) = employee {
                    qb.push(" AND ").push(emp_col).push(" = ").push_bind(*e);
                }
                qb.push(")");
            }
            Clause::Unassigned => {
                qb.push("(").push(org_col).push(" IS NULL)");
            }
        }
    }
    qb.push(")");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sql(f: &ListFilter) -> String {
        let mut qb = QueryBuilder::<Postgres>::new("");
        push_where(&mut qb, f, "t.o", "t.e");
        qb.sql().to_string()
    }

    #[test]
    fn empty_is_false_and_all_is_true() {
        assert_eq!(sql(&ListFilter::default()), "FALSE");
        assert_eq!(sql(&ListFilter(vec![Clause::All])), "TRUE");
        assert_eq!(sql(&ListFilter(vec![Clause::OrgIn { orgs: vec![1], employee: None }, Clause::All])), "TRUE");
    }

    #[test]
    fn clauses_are_or_joined_and_values_are_bound() {
        let f = ListFilter(vec![
            Clause::OrgIn { orgs: vec![1, 2], employee: None },
            Clause::OrgIn { orgs: vec![3], employee: Some(9) },
            Clause::Unassigned,
        ]);
        assert_eq!(
            sql(&f),
            "((t.o = ANY($1)) OR (t.o = ANY($2) AND t.e = $3) OR (t.o IS NULL))"
        );
    }
}
