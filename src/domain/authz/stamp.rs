//! domain/authz/stamp.rs — データの担当スタンプ(設計書 §6.5・§6.6)。
//!
//! スタンプ = 「担当組織」と「担当者」の組。作成時に親から写す。DB に触れない純粋な関数だけを置く。

/// データの担当スタンプ。どちらかが `None` のものは「未完成」として扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OwnerStamp {
    /// 担当組織の ID。
    pub org_unit_id: Option<i64>,
    /// 担当者(社員)の ID。
    pub employee_id: Option<i64>,
}

impl OwnerStamp {
    /// 組織と担当者の両方が入ったスタンプを作る。
    pub fn new(org: i64, emp: i64) -> Self {
        Self { org_unit_id: Some(org), employee_id: Some(emp) }
    }

    /// 組織と担当者の両方が入っているか。
    pub fn is_complete(&self) -> bool {
        self.org_unit_id.is_some() && self.employee_id.is_some()
    }
}

/// 担当スタンプを持つもの(各ドメインのデータが実装する)。
pub trait Stamped {
    /// このデータの担当スタンプ。
    fn stamp(&self) -> OwnerStamp;
}

impl Stamped for OwnerStamp {
    fn stamp(&self) -> OwnerStamp {
        *self
    }
}

/// スタンプを決められなかった理由。
#[derive(Debug, PartialEq, Eq)]
pub enum StampError {
    /// 指定にも親にも、組織と担当者が揃った担当が無い(「顧客の担当を設定してください」)。
    NoOwner,
}

/// 新しいデータのスタンプを決める(設計書 §6.6)。
///
/// 「画面での指定」→「親(近い順)」の順に見て、**組織と担当者の両方が入っている最初のもの**を採用する。
/// 片方しか入っていない候補は飛ばす(担当未設定の穴を作らないため)。
/// 自動取込の「受け皿」(D16)は、呼び出し側が最後の親として渡す。
pub fn resolve_stamp(explicit: Option<OwnerStamp>, parents: &[OwnerStamp]) -> Result<OwnerStamp, StampError> {
    explicit.iter().chain(parents.iter()).find(|c| c.is_complete()).copied().ok_or(StampError::NoOwner)
}

/// 担当を決められないデータの作成を拒否する強制を有効にするか(設計書 §2.3 の U10)。
///
/// PR 7a で有効化。`false` に戻すと、手動作成も担当なし(NULL)で作る(移行期間の挙動)。唯一のスイッチ。
pub const STAMP_REJECT_ENABLED: bool = true; // PR 7a で有効化

/// データを作る経路の種類。担当を決められないときの扱いが違う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampMode {
    /// 画面・APIからの手動作成。担当を決められなければ拒否する([`NoOwnerError`])。
    Manual,
    /// 自動取込(EDI 同期・メール取込・受注からの契約自動作成)。拒否すると取込が止まるため、
    /// 決められなければ担当なし(NULL)で作り、エラーログを残す。
    AutoImport,
}

impl StampMode {
    /// 自動取込か。
    pub fn is_auto_import(self) -> bool {
        self == StampMode::AutoImport
    }
}

/// 新しいデータのスタンプを決める。作成の経路から呼ぶ入口。
///
/// `resolve_stamp` と同じ規則(指定 → 親を近い順)。決められないとき:
/// - [`StampMode::Manual`]: [`STAMP_REJECT_ENABLED`] が `true` なら `Err(NoOwner)`(`false` の間は担当なしで作る)
/// - [`StampMode::AutoImport`]: 常にエラーにせず担当なし(両方 `None`)を返す。ログは呼び出し側で出す
pub fn stamp_for(mode: StampMode, explicit: Option<OwnerStamp>, parents: &[OwnerStamp]) -> Result<OwnerStamp, StampError> {
    stamp_for_with(STAMP_REJECT_ENABLED, mode, explicit, parents)
}

/// [`stamp_for`] の本体。拒否の有効・無効を引数で受けて、両方の挙動をテストできるようにする。
fn stamp_for_with(reject: bool, mode: StampMode, explicit: Option<OwnerStamp>, parents: &[OwnerStamp]) -> Result<OwnerStamp, StampError> {
    match resolve_stamp(explicit, parents) {
        Ok(s) => Ok(s),
        Err(e) if mode == StampMode::Manual && reject => Err(e),
        Err(_) => Ok(OwnerStamp::default()),
    }
}

/// 担当が決まらないときに利用者へ返す文言(HTTP 400 の `error`)。
pub const NO_OWNER_MESSAGE: &str = "担当が決まりません。先に取引先(顧客)の担当を設定してください(取引先の詳細 →「担当を変更」)";

/// 一括処理で、担当が決まらない行を作らずに続けたときの案内(応答の `message`)。
pub const SKIPPED_NO_OWNER_NOTE: &str = "担当が決まらないため作らなかった行があります。取引先の担当を設定してください";

/// 手動作成で担当を決められなかったことを表す型付きエラー。
/// `sqlx::Error` / `anyhow::Error` に包まれて上がってくるので、[`is_no_owner`] で見つける(文字列比較はしない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoOwnerError;

impl std::fmt::Display for NoOwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(NO_OWNER_MESSAGE)
    }
}

impl std::error::Error for NoOwnerError {}

/// エラーの原因の連鎖(`source()`)のどこかに [`NoOwnerError`] があるか。
/// `anyhow::Error` は `e.as_ref()`、`sqlx::Error` などはそのまま渡せる。
pub fn is_no_owner(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(c) = cur {
        if c.downcast_ref::<NoOwnerError>().is_some() {
            return true;
        }
        cur = c.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(o: i64, e: i64) -> OwnerStamp {
        OwnerStamp::new(o, e)
    }

    #[test]
    fn project_is_used_when_present() {
        // 契約: 指定なし → 案件 → 顧客
        assert_eq!(resolve_stamp(None, &[s(1, 10), s(2, 20)]), Ok(s(1, 10)));
    }

    #[test]
    fn received_order_without_project_uses_client() {
        // 案件を伴わない受注: 顧客の担当を写す
        let project_missing = OwnerStamp::default();
        assert_eq!(resolve_stamp(None, &[project_missing, s(2, 20)]), Ok(s(2, 20)));
    }

    #[test]
    fn invoice_chain_order() {
        // 請求書: 受注 → 案件 → 顧客
        let (order, project, client) = (s(1, 1), s(2, 2), s(3, 3));
        assert_eq!(resolve_stamp(None, &[order, project, client]), Ok(order));
        assert_eq!(resolve_stamp(None, &[OwnerStamp::default(), project, client]), Ok(project));
        assert_eq!(resolve_stamp(None, &[OwnerStamp::default(), OwnerStamp::default(), client]), Ok(client));
    }

    #[test]
    fn receiver_as_last_parent() {
        let receiver = s(9, 90);
        assert_eq!(resolve_stamp(None, &[OwnerStamp::default(), receiver]), Ok(receiver));
    }

    #[test]
    fn nothing_complete_is_no_owner() {
        assert_eq!(resolve_stamp(None, &[]), Err(StampError::NoOwner));
        assert_eq!(resolve_stamp(None, &[OwnerStamp::default()]), Err(StampError::NoOwner));
    }

    #[test]
    fn incomplete_candidates_are_skipped() {
        let only_org = OwnerStamp { org_unit_id: Some(1), employee_id: None };
        let only_emp = OwnerStamp { org_unit_id: None, employee_id: Some(1) };
        assert_eq!(resolve_stamp(Some(only_org), &[only_emp, s(5, 50)]), Ok(s(5, 50)));
        assert_eq!(resolve_stamp(Some(only_org), &[only_emp]), Err(StampError::NoOwner));
    }

    #[test]
    fn explicit_wins_when_complete() {
        assert_eq!(resolve_stamp(Some(s(7, 70)), &[s(1, 10)]), Ok(s(7, 70)));
    }

    #[test]
    fn manual_is_rejected_when_nothing_complete() {
        assert_eq!(stamp_for_with(true, StampMode::Manual, None, &[]), Err(StampError::NoOwner));
        assert_eq!(stamp_for_with(true, StampMode::Manual, None, &[OwnerStamp::default()]), Err(StampError::NoOwner));
        assert_eq!(stamp_for(StampMode::Manual, None, &[]), Err(StampError::NoOwner), "強制は有効");
    }

    #[test]
    fn manual_unassigned_when_switch_off() {
        assert_eq!(stamp_for_with(false, StampMode::Manual, None, &[]), Ok(OwnerStamp::default()));
    }

    #[test]
    fn auto_import_never_rejects() {
        assert_eq!(stamp_for(StampMode::AutoImport, None, &[]), Ok(OwnerStamp::default()));
        assert_eq!(stamp_for(StampMode::AutoImport, None, &[OwnerStamp::default()]), Ok(OwnerStamp::default()));
        assert_eq!(stamp_for_with(true, StampMode::AutoImport, None, &[]), Ok(OwnerStamp::default()));
    }

    #[test]
    fn auto_import_uses_receiver_when_parents_empty() {
        assert_eq!(stamp_for(StampMode::AutoImport, None, &[OwnerStamp::default(), s(9, 90)]), Ok(s(9, 90)));
    }

    #[test]
    fn resolved_stamp_is_used_in_both_modes() {
        assert_eq!(stamp_for(StampMode::Manual, None, &[s(1, 10)]), Ok(s(1, 10)));
        assert_eq!(stamp_for(StampMode::AutoImport, None, &[s(1, 10)]), Ok(s(1, 10)));
        assert_eq!(stamp_for(StampMode::Manual, Some(s(7, 70)), &[s(1, 10)]), Ok(s(7, 70)));
    }

    #[test]
    fn no_owner_is_detected_through_wrappers() {
        let direct: Box<dyn std::error::Error + Send + Sync> = Box::new(NoOwnerError);
        assert!(is_no_owner(direct.as_ref()));
        let sqlx_err = sqlx::Error::Decode(Box::new(NoOwnerError));
        assert!(is_no_owner(&sqlx_err));
        let wrapped = anyhow::Error::from(sqlx_err).context("案件の作成に失敗");
        assert!(is_no_owner(wrapped.as_ref()));
        let other = anyhow::Error::from(sqlx::Error::RowNotFound);
        assert!(!is_no_owner(other.as_ref()));
        assert!(!is_no_owner(&std::fmt::Error));
    }

    #[test]
    fn no_owner_message_is_shown() {
        assert_eq!(NoOwnerError.to_string(), NO_OWNER_MESSAGE);
    }
}
