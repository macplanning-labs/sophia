//! domain/authz/org_tree.rs — 組織の木(設計書 §6.2)。
//!
//! 組織は1本の木(ルートは1つ)。深さは最大 `MAX_ORG_DEPTH` 段。DB に触れない純粋な構造体。

use std::collections::{HashMap, HashSet};

/// 組織の深さの上限(ルートだけで1段と数える)。
pub const MAX_ORG_DEPTH: usize = 8;

/// 組織の木の検証・操作で起きるエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrgTreeError {
    /// ルート(親なし)が無い。
    NoRoot,
    /// ルートが2つ以上ある。
    MultipleRoots,
    /// 存在しない親を指している(値は組織ID)。
    UnknownParent(i64),
    /// 循環している(値は循環に含まれる組織ID)。
    Cycle(i64),
    /// 深さの上限を超える(値は超えた組織ID)。
    TooDeep(i64),
    /// 存在しない組織を指定した。
    UnknownNode(i64),
    /// 付け替えると、自分の配下を親にしてしまう(循環)。または、ルートを動かそうとした。
    WouldCreateCycle,
    /// 同じ組織IDが重複している。
    DuplicateId(i64),
}

/// 検証済みの組織の木。
#[derive(Debug, Clone)]
pub struct OrgTree {
    root: i64,
    parent: HashMap<i64, Option<i64>>,
    children: HashMap<i64, Vec<i64>>,
    depth: HashMap<i64, usize>,
}

impl OrgTree {
    /// `(組織ID, 親の組織ID)` の一覧から木を作る。ID重複・ルートが1つでない・親不明・循環・深さ超過はエラー。
    pub fn new(units: &[(i64, Option<i64>)]) -> Result<Self, OrgTreeError> {
        let mut parent = HashMap::new();
        for &(id, p) in units {
            if parent.insert(id, p).is_some() {
                return Err(OrgTreeError::DuplicateId(id));
            }
        }
        let mut roots = units.iter().filter(|(_, p)| p.is_none()).map(|(id, _)| *id);
        let root = roots.next().ok_or(OrgTreeError::NoRoot)?;
        if roots.next().is_some() {
            return Err(OrgTreeError::MultipleRoots);
        }
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(id, p) in units {
            if let Some(p) = p {
                if !parent.contains_key(&p) {
                    return Err(OrgTreeError::UnknownParent(id));
                }
                children.entry(p).or_default().push(id);
            }
        }
        // ルートから幅優先で辿り、深さを求める。辿れなかった組織は循環の中にいる。
        let mut depth = HashMap::new();
        depth.insert(root, 1usize);
        let mut queue = vec![root];
        while let Some(n) = queue.pop() {
            let d = depth.get(&n).copied().unwrap_or(1);
            for &c in children.get(&n).map(|v| v.as_slice()).unwrap_or(&[]) {
                if d + 1 > MAX_ORG_DEPTH {
                    return Err(OrgTreeError::TooDeep(c));
                }
                depth.insert(c, d + 1);
                queue.push(c);
            }
        }
        if let Some(&(id, _)) = units.iter().find(|(id, _)| !depth.contains_key(id)) {
            return Err(OrgTreeError::Cycle(id));
        }
        for v in children.values_mut() {
            v.sort_unstable();
        }
        Ok(Self { root, parent, children, depth })
    }

    /// ルート組織のID。
    pub fn root_id(&self) -> i64 {
        self.root
    }

    /// 組織が木に存在するか。
    pub fn contains(&self, id: i64) -> bool {
        self.parent.contains_key(&id)
    }

    /// `node` が `ancestor` 自身、またはその配下か。どちらかが未知なら false(拒否側)。
    pub fn is_self_or_descendant(&self, node: i64, ancestor: i64) -> bool {
        if !self.contains(node) || !self.contains(ancestor) {
            return false;
        }
        let mut cur = Some(node);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.parent.get(&c).copied().flatten();
        }
        false
    }

    /// `root` とその配下すべての組織ID(昇順)。未知なら空。
    pub fn subtree_ids(&self, root: i64) -> Vec<i64> {
        if !self.contains(root) {
            return Vec::new();
        }
        let mut out = vec![root];
        let mut i = 0;
        while i < out.len() {
            if let Some(cs) = self.children.get(&out[i]) {
                out.extend(cs.iter().copied());
            }
            i += 1;
        }
        out.sort_unstable();
        out
    }

    /// `node` を `new_parent` の下へ付け替えてよいか。
    /// 未知の組織・ルートの移動・自分の配下への付け替え(循環)・移動後に深さの上限を超える場合は拒否。
    pub fn can_reparent(&self, node: i64, new_parent: i64) -> Result<(), OrgTreeError> {
        for id in [node, new_parent] {
            if !self.contains(id) {
                return Err(OrgTreeError::UnknownNode(id));
            }
        }
        // ルートは動かせない(木が2本になる・ルートが無くなるのを防ぐ)。
        if node == self.root || self.is_self_or_descendant(new_parent, node) {
            return Err(OrgTreeError::WouldCreateCycle);
        }
        // 移動する部分木の高さ(node を1段と数える)を求め、新しい位置で上限を超えないか見る。
        let ids: HashSet<i64> = self.subtree_ids(node).into_iter().collect();
        let node_depth = self.depth.get(&node).copied().unwrap_or(1);
        let height = ids.iter().filter_map(|i| self.depth.get(i)).max().copied().unwrap_or(node_depth) - node_depth + 1;
        let new_parent_depth = self.depth.get(&new_parent).copied().unwrap_or(1);
        if new_parent_depth + height > MAX_ORG_DEPTH {
            return Err(OrgTreeError::TooDeep(node));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OrgTree {
        // 1 > 2 > {3 > 31, 4}, 1 > 5 > 6
        OrgTree::new(&[(1, None), (2, Some(1)), (3, Some(2)), (31, Some(3)), (4, Some(2)), (5, Some(1)), (6, Some(5))]).unwrap()
    }

    #[test]
    fn builds_and_queries() {
        let t = sample();
        assert_eq!(t.root_id(), 1);
        assert!(t.contains(31) && !t.contains(99));
        assert!(t.is_self_or_descendant(31, 3));
        assert!(t.is_self_or_descendant(3, 3));
        assert!(!t.is_self_or_descendant(3, 31));
        assert!(!t.is_self_or_descendant(4, 3));
        assert!(!t.is_self_or_descendant(99, 1) && !t.is_self_or_descendant(1, 99));
        assert_eq!(t.subtree_ids(2), vec![2, 3, 4, 31]);
        assert_eq!(t.subtree_ids(6), vec![6]);
        assert!(t.subtree_ids(99).is_empty());
    }

    #[test]
    fn construction_errors() {
        assert_eq!(OrgTree::new(&[]).unwrap_err(), OrgTreeError::NoRoot);
        assert_eq!(OrgTree::new(&[(1, None), (2, None)]).unwrap_err(), OrgTreeError::MultipleRoots);
        assert_eq!(OrgTree::new(&[(1, None), (2, Some(9))]).unwrap_err(), OrgTreeError::UnknownParent(2));
        assert_eq!(OrgTree::new(&[(1, None), (1, Some(1))]).unwrap_err(), OrgTreeError::DuplicateId(1));
        assert!(matches!(OrgTree::new(&[(1, None), (2, Some(3)), (3, Some(2))]), Err(OrgTreeError::Cycle(_))));
    }

    #[test]
    fn depth_limit() {
        let ok: Vec<_> = (1..=MAX_ORG_DEPTH as i64).map(|i| (i, if i == 1 { None } else { Some(i - 1) })).collect();
        assert!(OrgTree::new(&ok).is_ok());
        let mut too_deep = ok.clone();
        too_deep.push((100, Some(MAX_ORG_DEPTH as i64)));
        assert_eq!(OrgTree::new(&too_deep).unwrap_err(), OrgTreeError::TooDeep(100));
    }

    #[test]
    fn reparent_rules() {
        let t = sample();
        assert_eq!(t.can_reparent(4, 5), Ok(()));
        assert_eq!(t.can_reparent(99, 5), Err(OrgTreeError::UnknownNode(99)));
        assert_eq!(t.can_reparent(4, 99), Err(OrgTreeError::UnknownNode(99)));
        assert_eq!(t.can_reparent(2, 31), Err(OrgTreeError::WouldCreateCycle));
        assert_eq!(t.can_reparent(2, 2), Err(OrgTreeError::WouldCreateCycle));
        assert_eq!(t.can_reparent(1, 5), Err(OrgTreeError::WouldCreateCycle));
    }

    #[test]
    fn reparent_depth_limit() {
        // 鎖 1>2>...>7 と、別の枝 100>101(部分木の高さ2)。
        let mut u: Vec<(i64, Option<i64>)> = (1..=7).map(|i| (i, if i == 1 { None } else { Some(i - 1) })).collect();
        u.push((100, Some(1)));
        u.push((101, Some(100)));
        let t = OrgTree::new(&u).unwrap();
        // 7(深さ7)の下に高さ2の部分木 → 深さ9 で超過
        assert_eq!(t.can_reparent(100, 7), Err(OrgTreeError::TooDeep(100)));
        // 6(深さ6)の下なら深さ8 で収まる
        assert_eq!(t.can_reparent(100, 6), Ok(()));
    }
}
