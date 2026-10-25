//! domain/services/user_deactivation.rs — ユーザーを無効にしてよいかの判定(DEMO-000186)。
//!
//! 背景: 2026-10-09、本番で管理者が自分自身を無効にしてログアウトし、操作できなくなった。
//! 自分自身の無効化と、最後の有効な管理者の無効化を、サーバ側で拒否する。DB に触れない純粋な関数。

/// 無効化を拒否する理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeactivationRefusal {
    /// 自分自身を無効にしようとした。
    SelfDeactivation,
    /// 有効な管理者が、この人しか残っていない。
    LastActiveAdmin,
}

impl DeactivationRefusal {
    /// 画面に出す日本語のメッセージ。
    pub fn message(self) -> &'static str {
        match self {
            DeactivationRefusal::SelfDeactivation => "自分自身のアカウントは無効にできません(ログアウトして、操作できなくなるため)",
            DeactivationRefusal::LastActiveAdmin => "有効な管理者が1人だけのため、このアカウントは無効にできません。先に別の管理者を有効にしてください",
        }
    }
}

impl std::fmt::Display for DeactivationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for DeactivationRefusal {}

/// 有効なユーザーを無効にしてよいか。
/// - `actor_id`: 操作する管理者のユーザーID
/// - `target_id`: 無効にするユーザーのID
/// - `target_is_staff`: 無効にするユーザーが管理者か
/// - `other_active_staff`: 無効にするユーザー以外の、有効な管理者の人数
pub fn check_deactivation(actor_id: i64, target_id: i64, target_is_staff: bool, other_active_staff: i64) -> Result<(), DeactivationRefusal> {
    if actor_id == target_id {
        return Err(DeactivationRefusal::SelfDeactivation);
    }
    if target_is_staff && other_active_staff <= 0 {
        return Err(DeactivationRefusal::LastActiveAdmin);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_self_deactivation() {
        // 管理者が自分を無効にするのは、他に管理者がいても拒否する
        assert_eq!(check_deactivation(6, 6, true, 3), Err(DeactivationRefusal::SelfDeactivation));
    }

    #[test]
    fn refuses_the_last_active_admin() {
        assert_eq!(check_deactivation(1, 6, true, 0), Err(DeactivationRefusal::LastActiveAdmin));
    }

    #[test]
    fn allows_other_users() {
        assert_eq!(check_deactivation(1, 6, true, 1), Ok(()));
        // 一般ユーザーは、管理者の人数に関係なく無効にできる
        assert_eq!(check_deactivation(1, 7, false, 0), Ok(()));
    }
}
