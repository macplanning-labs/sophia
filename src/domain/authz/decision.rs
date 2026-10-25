//! domain/authz/decision.rs — 見える範囲の判定(設計書 §7.3、詳細設計書 §2)。
//!
//! 割り当て1行 × 権限1つ を `Grant`(組織 + スコープ)として持ち、判定は1行ずつ行う。
//! 行をまたいで、ロールと組織を混ぜない(P4)。DB には触れない純粋なロジック。
//!
//! 「本人の個人データ」(設計書 §7.3-2)の判定はここに無い。ルート表の `Own` として、
//! ハンドラ側が `employee_id` の一致で行う(詳細設計書 §2.4)。`Authz` はスタンプ型のデータだけを扱う。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::org_tree::OrgTree;
use super::permission::Permission;
use super::scope::DataScope;
use super::stamp::{OwnerStamp, Stamped};

/// 「担当未設定」規則(設計書 §7.3-4)を有効にするか。
/// PR 7b(担当の NOT NULL 化、マイグレーション 073)で false にした(設計書 §2.3 U9)。
/// 担当未設定のデータ自体が無くなるので、結果は変わらない。`scope.unassigned` の権限の行は消さない
/// (会社が変えた権限セットを上書きしない原則)。
pub const UNASSIGNED_RULE_ENABLED: bool = false;

/// 割り当て1行 × 権限1つ。`org_unit_id` はその割り当ての組織(判定の文脈)。行は決して統合しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    /// 割り当ての組織。
    pub org_unit_id: i64,
    /// その行のロールが、この権限に持つスコープ。
    pub scope: DataScope,
}

/// ユーザーの割り当て1行(組織 × ロール)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentRow {
    /// 割り当ての組織ID。
    pub org_unit_id: i64,
    /// 割り当てのロールID。
    pub role_id: i64,
}

/// ロールが持つ権限1つ(DB の `m_role_permission` の1行)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePermissionRow {
    /// ロールID。
    pub role_id: i64,
    /// 権限コード(例: "contract:edit")。
    pub permission_code: String,
    /// スコープの文字列("PRIVATE" など)。
    pub scope: String,
}

/// 認可エラー。
#[derive(Debug, PartialEq, Eq)]
pub enum AuthzError {
    /// 許可されていない。
    Forbidden,
}

/// ユーザー1人分の認可の判定器。
#[derive(Debug, Clone)]
pub struct Authz {
    pub(super) employee_id: Option<i64>,
    pub(super) is_staff: bool,
    pub(super) grants: HashMap<Permission, Vec<Grant>>,
    /// 「担当未設定」を許す権限(同じロールが権限と `scope.unassigned` の両方を持つ行がある)。
    pub(super) unassigned_ok: HashSet<Permission>,
    pub(super) tree: Arc<OrgTree>,
}

impl Authz {
    /// 割り当てとロールの権限から判定器を作る。
    /// DB にあるが一覧に無い権限コード・スコープ、木に無い組織への割り当ては無視する(拒否側に倒す)。
    pub fn build(
        employee_id: Option<i64>,
        is_staff: bool,
        assignments: &[AssignmentRow],
        role_perms: &[RolePermissionRow],
        tree: Arc<OrgTree>,
    ) -> Authz {
        let mut grants: HashMap<Permission, Vec<Grant>> = HashMap::new();
        let mut unassigned_ok = HashSet::new();
        let mut warned: HashSet<&str> = HashSet::new();
        for a in assignments.iter().filter(|a| tree.contains(a.org_unit_id)) {
            // この行のロールが持つ権限だけを集める(行をまたいで混ぜない)。
            let mut parsed: Vec<(Permission, DataScope)> = Vec::new();
            for rp in role_perms.iter().filter(|rp| rp.role_id == a.role_id) {
                match (Permission::from_code(&rp.permission_code), DataScope::from_db(&rp.scope)) {
                    (Some(p), Some(s)) => parsed.push((p, s)),
                    _ => {
                        // リクエストごとに組み立てるため、ここでは debug に留める。
                        // DB に未知のコードが残っていないかは、権限セットの保存時の検証(validate_role_permissions)で防ぐ。
                        if warned.insert(rp.permission_code.as_str()) {
                            tracing::debug!(code = %rp.permission_code, scope = %rp.scope, "未知の権限コードまたはスコープを無視しました");
                        }
                    }
                }
            }
            let role_has_unassigned = parsed.iter().any(|(p, _)| *p == Permission::ScopeUnassigned);
            for (p, scope) in parsed {
                grants.entry(p).or_default().push(Grant { org_unit_id: a.org_unit_id, scope });
                // 担当未設定の規則: その行のロールが、権限と scope.unassigned の両方を持つこと(§7.3-4)。
                if role_has_unassigned {
                    unassigned_ok.insert(p);
                }
            }
        }
        Authz { employee_id, is_staff, grants, unassigned_ok, tree }
    }

    /// 管理者(is_staff)か。
    pub fn is_staff(&self) -> bool {
        self.is_staff
    }

    /// 本人の社員ID。
    pub fn employee_id(&self) -> Option<i64> {
        self.employee_id
    }

    /// 組織の木。
    pub(super) fn tree(&self) -> &OrgTree {
        &self.tree
    }

    /// 権限 `perm` の割り当て行ごとの Grant。
    pub(super) fn grants_of(&self, perm: Permission) -> &[Grant] {
        self.grants.get(&perm).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 「担当未設定」のデータを許す規則が、この権限に当てはまるか。
    pub(super) fn unassigned_applies(&self, perm: Permission) -> bool {
        UNASSIGNED_RULE_ENABLED && self.unassigned_ok.contains(&perm)
    }

    /// 権限を1つでも持つか(管理者は常に持つ)。
    pub fn has_any(&self, perm: Permission) -> bool {
        self.is_staff || !self.grants_of(perm).is_empty()
    }

    /// 権限 `perm` を全社(GLOBAL)のスコープで持つか(管理者は常に持つ)。
    /// 「全員分を見てよいか」(給与・社員情報・経費など、スタンプの無い領域)の判定に使う。
    pub fn has_global(&self, perm: Permission) -> bool {
        self.is_staff || self.grants_of(perm).iter().any(|g| g.scope == DataScope::Global)
    }

    /// ロールの割り当てで、権限を全社(GLOBAL)で持つか。管理者であることは数えない。
    /// 給与明細のように、管理者の権限とは分けて決める領域で使う(DEMO-000167 項目5の決まり)。
    pub fn has_global_grant(&self, perm: Permission) -> bool {
        self.grants_of(perm).iter().any(|g| g.scope == DataScope::Global)
    }

    /// スタンプの無いデータ(マスタ・受信メール・他人の個人データ等)の判定。権限があるかだけで決まる。
    pub fn can_unstamped(&self, perm: Permission) -> bool {
        self.has_any(perm)
    }

    /// 権限 `perm` で、データ `target` を扱ってよいか(設計書 §7.3 の 1・3・4・5)。
    pub fn can(&self, perm: Permission, target: &impl Stamped) -> bool {
        if self.is_staff {
            return true;
        }
        if !perm.is_stamped() {
            return self.can_unstamped(perm);
        }
        let stamp = target.stamp();
        // 割り当て1行ずつ、その行の組織とスコープだけで判定し、1行でも満たせば許可(和集合)。
        if self.grants_of(perm).iter().any(|g| self.matches(g, &stamp)) {
            return true;
        }
        // 担当未設定のデータ(移行期間のみ)。
        stamp.org_unit_id.is_none() && self.unassigned_applies(perm)
    }

    /// 1行の Grant がスタンプを許すか(設計書 §7.2 の表)。
    fn matches(&self, g: &Grant, stamp: &OwnerStamp) -> bool {
        match g.scope {
            DataScope::Global => true,
            DataScope::Hierarchical => stamp.org_unit_id.is_some_and(|o| self.tree.is_self_or_descendant(o, g.org_unit_id)),
            DataScope::Unit => stamp.org_unit_id == Some(g.org_unit_id),
            // 本人限定は「担当者が自分」かつ「担当組織が割り当ての組織の配下」(文脈の外の本人分は許さない。U12)。
            DataScope::Private => {
                self.employee_id.is_some()
                    && stamp.employee_id == self.employee_id
                    && stamp.org_unit_id.is_some_and(|o| self.tree.is_self_or_descendant(o, g.org_unit_id))
            }
        }
    }

    /// 許可されなければ `Forbidden`。
    pub fn require(&self, perm: Permission, target: &impl Stamped) -> Result<(), AuthzError> {
        if self.can(perm, target) {
            Ok(())
        } else {
            Err(AuthzError::Forbidden)
        }
    }

    /// 新規作成時に指定するスタンプが許されるか。
    /// 組織と担当者が揃っていて、かつ作成者自身がその範囲を扱える場合のみ(範囲外に押し込めない。設計書 §6.6)。
    pub fn can_stamp_new(&self, perm: Permission, stamp: &OwnerStamp) -> bool {
        stamp.is_complete() && self.can(perm, stamp)
    }
}

/// 割り当て・権限セットの検証で拒否する理由。
#[derive(Debug, PartialEq, Eq)]
pub enum AssignmentError {
    /// 組織が木に存在しない。
    UnknownOrg,
    /// スタンプ型の権限に GLOBAL を持つロールを、ルート以外の組織に割り当てようとした。
    GlobalStampedOutsideRoot,
    /// 一覧に無い権限コード、または不明なスコープ(値はコード)。
    UnknownCodeOrScope(String),
    /// スタンプを持たない権限に、GLOBAL 以外のスコープを付けた(範囲を判定に使わないため、黙って無視されてしまう。値はコード)。
    NonGlobalScopeOnUnstamped(String),
}

/// ロールを組織に割り当ててよいか検証する(設計書 §6.2 ④)。
/// スタンプ型データの GLOBAL は組織の枠を超えるため、ルート組織への割り当てに限る。
/// スタンプを持たない権限の GLOBAL は組織を問わない。
pub fn validate_assignment(role_perms_of_role: &[RolePermissionRow], org_unit_id: i64, tree: &OrgTree) -> Result<(), AssignmentError> {
    if !tree.contains(org_unit_id) {
        return Err(AssignmentError::UnknownOrg);
    }
    let has_global_stamped = role_perms_of_role.iter().any(|rp| {
        DataScope::from_db(&rp.scope) == Some(DataScope::Global) && Permission::from_code(&rp.permission_code).is_some_and(|p| p.is_stamped())
    });
    if has_global_stamped && org_unit_id != tree.root_id() {
        return Err(AssignmentError::GlobalStampedOutsideRoot);
    }
    Ok(())
}

/// ロールの権限セットを保存してよいか検証する(管理画面で権限セットを変えるとき。PR 5)。
/// `assigned_org_ids` には、このロールが今割り当てられている組織をすべて渡す。
/// 後からスタンプ型の権限に GLOBAL を足すと、課に割り当て済みの人が全社を見られてしまうため、既存の割り当ても検査し直す。
pub fn validate_role_permissions(
    role_perms_of_role: &[RolePermissionRow],
    assigned_org_ids: &[i64],
    tree: &OrgTree,
) -> Result<(), AssignmentError> {
    for rp in role_perms_of_role {
        let (Some(p), Some(scope)) = (Permission::from_code(&rp.permission_code), DataScope::from_db(&rp.scope)) else {
            return Err(AssignmentError::UnknownCodeOrScope(rp.permission_code.clone()));
        };
        if !p.is_stamped() && scope != DataScope::Global {
            return Err(AssignmentError::NonGlobalScopeOnUnstamped(rp.permission_code.clone()));
        }
    }
    for &org in assigned_org_ids {
        validate_assignment(role_perms_of_role, org, tree)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
