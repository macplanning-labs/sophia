/// infrastructure/repositories/jwt_blacklist_repo.rs — JWT黒リスト管理(s_jwt_blacklist) CRUD
use async_trait::async_trait;
use auth_core::domain::jwt::TokenBlacklist;
use auth_core::error::AuthError;
use chrono::{DateTime, Utc};

/// JWT JTI（JWT ID）の黒リストを管理するリポジトリ
/// ログアウト時にトークンを即座に無効化する
pub struct JwtBlacklistRepo(pub sqlx::PgPool);

#[async_trait]
impl TokenBlacklist for JwtBlacklistRepo {
    async fn blacklist(&self, jti: &str, expires_at: DateTime<Utc>) -> Result<(), AuthError> {
        // Parse jti string into UUID; return AuthError::Internal if parsing fails
        let jti_uuid = uuid::Uuid::parse_str(jti)
            .map_err(|e| AuthError::Internal(format!("Failed to parse JTI as UUID: {}", e)))?;

        // INSERT with ON CONFLICT DO NOTHING to safely handle concurrent logout calls
        sqlx::query(
            "INSERT INTO s_jwt_blacklist (jti, expires_at) VALUES ($1, $2) ON CONFLICT (jti) DO NOTHING"
        )
        .bind(jti_uuid)
        .bind(expires_at)
        .execute(&self.0)
        .await
        .map_err(|e| AuthError::Internal(format!("Failed to blacklist JWT: {}", e)))?;

        Ok(())
    }

    async fn is_blacklisted(&self, jti: &str) -> Result<bool, AuthError> {
        // Try to parse jti as UUID; if parsing fails, treat as "not blacklisted"
        // since a malformed jti would not exist in the DB and already failed JWT decode
        let jti_uuid = match uuid::Uuid::parse_str(jti) {
            Ok(uuid) => uuid,
            Err(_) => return Ok(false), // Malformed JTI is not in the blacklist
        };

        // Check if JTI exists in blacklist
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM s_jwt_blacklist WHERE jti = $1)")
                .bind(jti_uuid)
                .fetch_one(&self.0)
                .await
                .map_err(|e| {
                    AuthError::Internal(format!("Failed to check JWT blacklist: {}", e))
                })?;

        Ok(exists)
    }
}

impl JwtBlacklistRepo {
    /// 期限切れブラックリスト行を削除
    pub async fn purge_expired(&self) -> Result<u64, AuthError> {
        let r = sqlx::query("DELETE FROM s_jwt_blacklist WHERE expires_at < NOW()")
            .execute(&self.0)
            .await
            .map_err(|_| AuthError::Internal("Failed to purge JWT blacklist".into()))?;
        Ok(r.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::repositories::test_support;

    #[tokio::test]
    async fn test_blacklist_and_check_jwt() {
        test_support::with_rollback(|pool| async {
            let repo = JwtBlacklistRepo(pool);
            let jti = "550e8400-e29b-41d4-a716-446655440000";
            let expires_at = Utc::now() + chrono::Duration::hours(1);

            // Verify JTI is not blacklisted initially
            let not_blacklisted = repo.is_blacklisted(jti).await.unwrap();
            assert!(!not_blacklisted, "JTI should not be blacklisted initially");

            // Blacklist the JTI
            let result = repo.blacklist(jti, expires_at).await;
            assert!(result.is_ok(), "blacklist should succeed");

            // Verify it's now blacklisted
            let is_blacklisted = repo.is_blacklisted(jti).await.unwrap();
            assert!(
                is_blacklisted,
                "JTI should be blacklisted after blacklist call"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_non_blacklisted_jwt_returns_false() {
        test_support::with_rollback(|pool| async {
            let repo = JwtBlacklistRepo(pool);
            let jti = "550e8400-e29b-41d4-a716-446655440001";

            // Check that a non-existent JTI is not blacklisted
            let is_blacklisted = repo.is_blacklisted(jti).await.unwrap();
            assert!(
                !is_blacklisted,
                "Non-existent JTI should not be blacklisted"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_purge_expired() {
        test_support::with_rollback(|pool| async {
            let repo = JwtBlacklistRepo(pool);

            // Insert one expired and one valid JTI
            let expired_jti = "550e8400-e29b-41d4-a716-446655440099";
            let valid_jti = "550e8400-e29b-41d4-a716-446655440100";
            let expired_at = Utc::now() - chrono::Duration::hours(1);
            let valid_at = Utc::now() + chrono::Duration::hours(1);

            repo.blacklist(expired_jti, expired_at).await.unwrap();
            repo.blacklist(valid_jti, valid_at).await.unwrap();

            // Purge expired
            let deleted = repo.purge_expired().await.unwrap();
            assert_eq!(deleted, 1, "Should delete exactly one expired row");

            // Verify expired is gone, valid remains
            assert!(
                !repo.is_blacklisted(expired_jti).await.unwrap(),
                "Expired JTI should be gone"
            );
            assert!(
                repo.is_blacklisted(valid_jti).await.unwrap(),
                "Valid JTI should remain"
            );
        })
        .await;
    }
}
