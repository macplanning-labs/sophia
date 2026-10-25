/// infrastructure/repositories/authz_repo.rs — ユーザーの認可判定器(Authz)の組み立て

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use sqlx::PgPool;

use super::org_repo::OrgUnitRow;
use super::{org_assignment_repo, org_repo, role_repo};
use crate::domain::authz::decision::Authz;
use crate::domain::authz::org_tree::OrgTree;

/// ユーザーの `Authz` を DB から組み立てる。
///
/// リクエストごとに呼ぶ。キャッシュしない(割り当てを外した直後に権限が残るのを防ぐ。設計書 §7.5)。
pub async fn load_authz(pool: &PgPool, user_id: i64, employee_id: Option<i64>, is_staff: bool) -> Result<Authz> {
    let units = org_repo::list_all(pool).await?;
    let pairs: Vec<(i64, Option<i64>)> = units.iter().map(|u| (u.id, u.parent_id)).collect();
    let tree = OrgTree::new(&pairs).map_err(|e| anyhow!("組織の木を作れません(組織データが不正です): {:?}", e))?;

    // 無効な組織、または無効な組織の配下への割り当ては、権限として働かせない
    // (事業部を無効にしたら、その下の課の割り当ても止まる)。
    let disabled = disabled_org_ids(&units);
    let assignments: Vec<_> = org_assignment_repo::list_for_user(pool, user_id)
        .await?
        .into_iter()
        .filter(|a| !disabled.contains(&a.org_unit_id))
        .collect();
    let role_perms = if assignments.is_empty() {
        Vec::new()
    } else {
        let mut role_ids: Vec<i64> = assignments.iter().map(|a| a.role_id).collect();
        role_ids.sort_unstable();
        role_ids.dedup();
        role_repo::list_permissions_for_roles(pool, &role_ids).await?
    };
    Ok(Authz::build(employee_id, is_staff, &assignments, &role_perms, Arc::new(tree)))
}

/// 無効な組織と、その配下(子・孫…)の組織ID。
/// 木の検証(循環なし)は `OrgTree::new` が先に済ませている前提だが、念のため辿る回数を組織数で打ち切る。
pub fn disabled_org_ids(units: &[OrgUnitRow]) -> HashSet<i64> {
    let by_id: HashMap<i64, &OrgUnitRow> = units.iter().map(|u| (u.id, u)).collect();
    let mut disabled = HashSet::new();
    for u in units {
        let mut cur = Some(u);
        let mut steps = 0;
        while let Some(c) = cur {
            if !c.is_active {
                disabled.insert(u.id);
                break;
            }
            steps += 1;
            if steps > units.len() {
                break;
            }
            cur = c.parent_id.and_then(|p| by_id.get(&p).copied());
        }
    }
    disabled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: i64, parent: Option<i64>, active: bool) -> OrgUnitRow {
        OrgUnitRow { id, parent_id: parent, name: format!("u{id}"), unit_kind: None, sort_order: 0, is_active: active }
    }

    #[test]
    fn disabling_a_parent_disables_its_whole_branch() {
        // 全社(1) > 事業部A(2, 無効) > 1課(3) > 1係(31)、全社 > 事業部B(5) > 3課(6)
        let units = [
            unit(1, None, true),
            unit(2, Some(1), false),
            unit(3, Some(2), true),
            unit(31, Some(3), true),
            unit(5, Some(1), true),
            unit(6, Some(5), true),
        ];
        let d = disabled_org_ids(&units);
        assert_eq!(d, HashSet::from([2, 3, 31]));
    }

    #[test]
    fn all_active_disables_nothing() {
        let units = [unit(1, None, true), unit(2, Some(1), true)];
        assert!(disabled_org_ids(&units).is_empty());
    }
}
