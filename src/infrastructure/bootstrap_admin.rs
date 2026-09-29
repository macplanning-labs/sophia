//! 初期管理者の作成（起動時または CLI コマンドで実行）

use crate::domain::services::password_policy;
use std::fmt;

/// 初期管理者作成に必要な入力情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapInput {
    pub email: String,
    pub username: String,
    pub password: String,
}

/// 初期管理者作成のエラー理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapError {
    /// メール・パスワードの片方だけが指定されている
    Incomplete,
    /// メールアドレスの形式が不正
    InvalidEmail,
    /// パスワードが規則に合致していない
    WeakPassword(String),
    /// ユーザー名が長すぎる
    UsernameTooLong,
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BootstrapError::Incomplete => write!(f, "メールアドレスとパスワードは両方設定してください"),
            BootstrapError::InvalidEmail => write!(f, "メールアドレスの形式が不正です"),
            BootstrapError::WeakPassword(msg) => write!(f, "{}", msg),
            BootstrapError::UsernameTooLong => write!(f, "ユーザー名は150文字以内にしてください"),
        }
    }
}

/// 入力を検証し、BootstrapInput を構築する
///
/// - email・password がどちらも空（前後の空白を含む）→ `Ok(None)`（何も指定されていない）
/// - 片方だけ空 → `Err(Incomplete)`
/// - メール: 前後の空白を除去、`@` を1個だけ含み前後が空でなく、空白を含まず、255文字以内
/// - パスワード: password_policy::validate を呼ぶ（前後の空白は除去しない）
/// - ユーザー名: 空なら、メールの `@` より前を使う。150文字を超える場合は `Err(UsernameTooLong)`
pub fn parse_bootstrap_input(email: &str, password: &str, username: &str)
    -> Result<Option<BootstrapInput>, BootstrapError>
{
    let email_trimmed = email.trim();
    let password_stripped = password; // trim しない
    let username_input = username.trim();

    let email_empty = email_trimmed.is_empty();
    let password_empty = password_stripped.trim().is_empty(); // 判定は trim 後

    // 両方空 → Ok(None)
    if email_empty && password_empty {
        return Ok(None);
    }

    // 片方だけ空 → Err(Incomplete)
    if email_empty || password_empty {
        return Err(BootstrapError::Incomplete);
    }

    // メール検証: @ を1個だけ、前後が空でない、空白を含まない、255文字以内
    let at_count = email_trimmed.matches('@').count();
    if at_count != 1 {
        return Err(BootstrapError::InvalidEmail);
    }
    if email_trimmed.contains(' ') || email_trimmed.contains('\t') || email_trimmed.contains('\n') {
        return Err(BootstrapError::InvalidEmail);
    }
    let parts: Vec<&str> = email_trimmed.split('@').collect();
    if parts[0].is_empty() || parts[1].is_empty() {
        return Err(BootstrapError::InvalidEmail);
    }
    if email_trimmed.chars().count() > 255 {
        return Err(BootstrapError::InvalidEmail);
    }

    // パスワード検証: password_policy::validate を呼ぶ（trim しない）
    if let Err(msg) = password_policy::validate(password_stripped) {
        return Err(BootstrapError::WeakPassword(msg));
    }

    // ユーザー名: 空なら @ より前を使う、150文字超は エラー
    let computed_username = if username_input.is_empty() {
        email_trimmed.split('@').next().unwrap_or(email_trimmed).to_string()
    } else {
        username_input.to_string()
    };

    if computed_username.chars().count() > 150 {
        return Err(BootstrapError::UsernameTooLong);
    }

    Ok(Some(BootstrapInput {
        email: email_trimmed.to_string(),
        username: computed_username,
        password: password_stripped.to_string(),
    }))
}

/// ログ出力用にメールアドレスをマスクする
///
/// 形式: `a***@example.com` （先頭1文字とドメインのみ）
/// `@` が無ければ `***` を返す
pub fn mask_email(email: &str) -> String {
    match (email.find('@'), email.chars().next()) {
        (Some(at_pos), Some(first_char)) if at_pos > 0 => {
            format!("{}***{}", first_char, &email[at_pos..])
        }
        _ => "***".to_string(),
    }
}

/// 初期管理者作成の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 管理者を作成した
    Created,
    /// ユーザーが既に存在するため、スキップした
    SkippedUsersExist,
}

/// トランザクション内で初期管理者を作成する
///
/// 1. advisory lock で同時実行を直列化
/// 2. ユーザーが既にいれば SkippedUsersExist（ロールバック）
/// 3. パスワードをハッシュ化
/// 4. s_user, s_user_profile に INSERT
/// 5. コミット → Created
pub async fn create_first_admin(pool: &sqlx::PgPool, input: &BootstrapInput)
    -> anyhow::Result<Outcome>
{
    let mut tx = pool.begin().await?;

    // advisory lock で同時実行を直列化
    // 固定の i64 値: 0x534F_5048_4144_4D01 ("SOPHAD" + 0x01)
    const ADVISORY_LOCK: i64 = 0x534F_5048_4144_4D01i64;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ADVISORY_LOCK)
        .execute(&mut *tx)
        .await?;

    // ユーザーが既にいるか確認
    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM s_user)")
        .fetch_one(&mut *tx)
        .await?;

    if user_exists {
        // ロールバック（自動で tx が drop される）
        drop(tx);
        return Ok(Outcome::SkippedUsersExist);
    }

    // パスワードをハッシュ化
    let hashed_password = auth_core::domain::password::hash_password(&input.password)?;

    // s_user に INSERT
    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO s_user (email, password, username, is_staff, is_active, can_view_all_payroll, can_view_all_expenses) \
         VALUES ($1, $2, $3, TRUE, TRUE, TRUE, TRUE) RETURNING id"
    )
    .bind(&input.email)
    .bind(&hashed_password)
    .bind(&input.username)
    .fetch_one(&mut *tx)
    .await?;

    // s_user_profile に INSERT
    sqlx::query(
        "INSERT INTO s_user_profile (user_id, partner_id, employee_id, is_first_login) \
         VALUES ($1, NULL, NULL, FALSE)"
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    // コミット
    tx.commit().await?;

    Ok(Outcome::Created)
}

/// 環境変数から初期管理者を作成する
///
/// 環境変数:
/// - INITIAL_ADMIN_EMAIL
/// - INITIAL_ADMIN_PASSWORD
/// - INITIAL_ADMIN_USERNAME (オプション)
///
/// 両方未設定/空の場合は何もしない（DB への問い合わせもしない）。
/// 失敗してもサーバー起動は止めない（パニックしない）。
pub async fn run_from_env(pool: &sqlx::PgPool) {
    let email = std::env::var("INITIAL_ADMIN_EMAIL").unwrap_or_default();
    let password = std::env::var("INITIAL_ADMIN_PASSWORD").unwrap_or_default();
    let username = std::env::var("INITIAL_ADMIN_USERNAME").unwrap_or_default();

    let email_empty = email.trim().is_empty();
    let password_empty = password.trim().is_empty();

    // 両方未設定/空 → 何もしない
    if email_empty && password_empty {
        return;
    }

    // 片方のみ設定 → エラー
    if email_empty || password_empty {
        tracing::error!("INITIAL_ADMIN_EMAIL と INITIAL_ADMIN_PASSWORD は両方設定してください");
        return;
    }

    // 入力を検証(パスワードは入力どおり。前後の空白を除去するとログイン時の入力とずれる)
    match parse_bootstrap_input(&email, &password, &username) {
        Err(e) => {
            tracing::error!("初期管理者作成エラー: {}", e);
        }
        Ok(None) => {
            tracing::error!("INITIAL_ADMIN_EMAIL と INITIAL_ADMIN_PASSWORD は両方設定してください");
        }
        Ok(Some(input)) => {
            // create_first_admin を呼ぶ
            match create_first_admin(pool, &input).await {
                Ok(Outcome::Created) => {
                    tracing::info!("初期管理者を作成しました: {}", mask_email(&input.email));
                    tracing::info!("作成後は INITIAL_ADMIN_* を .env から削除し、docker compose up -d --force-recreate で再作成してください");
                }
                Ok(Outcome::SkippedUsersExist) => {
                    tracing::warn!("INITIAL_ADMIN_* が設定されたままです。不要なら削除してください");
                }
                Err(e) => {
                    tracing::error!("初期管理者作成エラー: {}", e);
                }
            }
        }
    }
}

/// コマンドラインで初期管理者を作成する
///
/// password_stdin が true なら標準入力から1行、そうでなければ環境変数 INITIAL_ADMIN_PASSWORD。
///
/// 戻り値: 作成 true、ユーザーが既にいる true、入力エラー false、DB エラー Err
pub async fn run_cli(pool: &sqlx::PgPool, email: &str, username: Option<&str>, password_stdin: bool)
    -> anyhow::Result<bool>
{
    // パスワードを取得
    let password = if password_stdin {
        // 標準入力から1行読む（末尾の改行 \r\n のみ除去）
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf)?;
        buf.trim_end_matches('\n').trim_end_matches('\r').to_string()
    } else {
        // 環境変数から
        std::env::var("INITIAL_ADMIN_PASSWORD")
            .map_err(|_| anyhow::anyhow!("パスワードは --password-stdin で指定するか、INITIAL_ADMIN_PASSWORD 環境変数を設定してください"))?
    };

    // 入力を検証
    let input = match parse_bootstrap_input(email, &password, username.unwrap_or("")) {
        Ok(Some(inp)) => inp,
        Ok(None) => {
            println!("メールアドレスとパスワードを指定してください");
            return Ok(false);
        }
        Err(e) => {
            println!("入力エラー: {}", e);
            return Ok(false);
        }
    };

    // 作成を試みる
    match create_first_admin(pool, &input).await {
        Ok(Outcome::Created) => {
            println!("初期管理者を作成しました: {}", input.email);
            Ok(true)
        }
        Ok(Outcome::SkippedUsersExist) => {
            println!("ユーザーが既に存在するため、何もしませんでした");
            Ok(true)
        }
        Err(e) => {
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_is_kept_exactly_including_spaces() {
        let given = " Abcd1234567890 ";
        let r = parse_bootstrap_input("a@example.com", given, "").unwrap().unwrap();
        assert_eq!(r.password, given);
    }

    #[test]
    fn test_mask_email_multibyte_does_not_panic() {
        assert_eq!(mask_email("管理者@example.com"), "管***@example.com");
        assert_eq!(mask_email("@example.com"), "***");
        assert_eq!(mask_email(""), "***");
    }

    #[test]
    fn test_username_length_counts_characters() {
        let long_ok = "あ".repeat(150);
        assert!(parse_bootstrap_input("a@example.com", "Abcd1234567890", &long_ok).unwrap().is_some());
        let too_long = "あ".repeat(151);
        assert_eq!(
            parse_bootstrap_input("a@example.com", "Abcd1234567890", &too_long),
            Err(BootstrapError::UsernameTooLong)
        );
    }

    // ── parse_bootstrap_input のテスト ──

    #[test]
    fn test_both_empty() {
        let result = parse_bootstrap_input("", "", "");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_both_whitespace() {
        let result = parse_bootstrap_input("   ", "  \t  ", "");
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_email_only() {
        let result = parse_bootstrap_input("test@example.com", "", "");
        assert!(matches!(result, Err(BootstrapError::Incomplete)));
    }

    #[test]
    fn test_password_only() {
        let result = parse_bootstrap_input("", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::Incomplete)));
    }

    #[test]
    fn test_no_at() {
        let result = parse_bootstrap_input("testexample.com", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_two_ats() {
        let result = parse_bootstrap_input("test@ex@mple.com", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_at_at_start() {
        let result = parse_bootstrap_input("@example.com", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_at_at_end() {
        let result = parse_bootstrap_input("test@", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_email_with_space() {
        let result = parse_bootstrap_input("test @example.com", "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_email_too_long() {
        let long_email = "a".repeat(256) + "@example.com";
        let result = parse_bootstrap_input(&long_email, "Abcd123456789", "");
        assert!(matches!(result, Err(BootstrapError::InvalidEmail)));
    }

    #[test]
    fn test_password_too_short() {
        let result = parse_bootstrap_input("test@example.com", "Abcd12345", "");
        assert!(matches!(result, Err(BootstrapError::WeakPassword(_))));
    }

    #[test]
    fn test_password_no_digit() {
        let result = parse_bootstrap_input("test@example.com", "Abcdefghijkl", "");
        assert!(matches!(result, Err(BootstrapError::WeakPassword(_))));
    }

    #[test]
    fn test_password_no_upper() {
        let result = parse_bootstrap_input("test@example.com", "abcdefghij12", "");
        assert!(matches!(result, Err(BootstrapError::WeakPassword(_))));
    }

    #[test]
    fn test_valid_input() {
        let result = parse_bootstrap_input("test@example.com", "Abcd123456789", "");
        assert!(result.is_ok());
        let input = result.unwrap().unwrap();
        assert_eq!(input.email, "test@example.com");
        assert_eq!(input.username, "test"); // @ より前
        let expected = "Abcd123456789";
        assert_eq!(input.password, expected);
    }

    #[test]
    fn test_valid_input_with_username() {
        let result = parse_bootstrap_input("test@example.com", "Abcd123456789", "MyName");
        assert!(result.is_ok());
        let input = result.unwrap().unwrap();
        assert_eq!(input.email, "test@example.com");
        assert_eq!(input.username, "MyName");
    }

    #[test]
    fn test_email_trimmed() {
        let result = parse_bootstrap_input("  test@example.com  ", "Abcd123456789", "");
        assert!(result.is_ok());
        let input = result.unwrap().unwrap();
        assert_eq!(input.email, "test@example.com");
    }

    #[test]
    fn test_password_not_trimmed() {
        // パスワードの前後の空白は trim しない
        let result = parse_bootstrap_input("test@example.com", "  Abcd123456789  ", "");
        // password_policy::validate は自動的に trim するため、これは有効なパスワードになる
        assert!(result.is_ok());
        let input = result.unwrap().unwrap();
        let expected = "  Abcd123456789  ";
        assert_eq!(input.password, expected);
    }

    #[test]
    fn test_username_too_long() {
        let long_name = "a".repeat(151);
        let result = parse_bootstrap_input("test@example.com", "Abcd123456789", &long_name);
        assert!(matches!(result, Err(BootstrapError::UsernameTooLong)));
    }

    #[test]
    fn test_username_exactly_150() {
        let name_150 = "a".repeat(150);
        let result = parse_bootstrap_input("test@example.com", "Abcd123456789", &name_150);
        assert!(result.is_ok());
    }

    #[test]
    fn test_case_preserved() {
        let result = parse_bootstrap_input("Test@Example.Com", "Abcd123456789", "");
        assert!(result.is_ok());
        let input = result.unwrap().unwrap();
        assert_eq!(input.email, "Test@Example.Com");
    }

    // ── mask_email のテスト ──

    #[test]
    fn test_mask_normal() {
        let result = mask_email("admin@example.com");
        assert_eq!(result, "a***@example.com");
    }

    #[test]
    fn test_mask_single_char() {
        let result = mask_email("a@example.com");
        assert_eq!(result, "a***@example.com");
    }

    #[test]
    fn test_mask_no_at() {
        let result = mask_email("plaintext");
        assert_eq!(result, "***");
    }

    #[test]
    fn test_mask_at_start() {
        let result = mask_email("@example.com");
        assert_eq!(result, "***");
    }

    // ── BootstrapError::Display ──

    #[test]
    fn test_display_incomplete() {
        let err = BootstrapError::Incomplete;
        assert_eq!(err.to_string(), "メールアドレスとパスワードは両方設定してください");
    }

    #[test]
    fn test_display_invalid_email() {
        let err = BootstrapError::InvalidEmail;
        assert_eq!(err.to_string(), "メールアドレスの形式が不正です");
    }

    #[test]
    fn test_display_weak_password() {
        let err = BootstrapError::WeakPassword("テスト".to_string());
        assert_eq!(err.to_string(), "テスト");
    }

    #[test]
    fn test_display_username_too_long() {
        let err = BootstrapError::UsernameTooLong;
        assert_eq!(err.to_string(), "ユーザー名は150文字以内にしてください");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;

    // 並行実行テスト用の mutex
    static DB_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn cleanup_users(pool: &sqlx::PgPool) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM s_user_profile").execute(pool).await?;
        sqlx::query("DELETE FROM s_user").execute(pool).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires database (TEST_DATABASE_URL)"]
    async fn test_db_create_first_admin() {
        let _lock = DB_TEST_LOCK.lock().await;

        let pool = create_test_pool().await.expect("Failed to create test pool");
        cleanup_users(&pool).await.expect("Failed to cleanup");

        let input = BootstrapInput {
            email: "admin@test.example.com".to_string(),
            username: "TestAdmin".to_string(),
            password: "TestPass123456".to_string(),
        };

        let result = create_first_admin(&pool, &input).await.expect("Failed to create admin");
        assert_eq!(result, Outcome::Created);

        // s_user を確認
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user")
            .fetch_one(&pool)
            .await
            .expect("Failed to count users");
        assert_eq!(user_count, 1);

        // s_user_profile を確認
        let profile_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user_profile")
            .fetch_one(&pool)
            .await
            .expect("Failed to count profiles");
        assert_eq!(profile_count, 1);

        // ユーザーの属性を確認
        let (is_staff, is_active, can_payroll, can_expenses, is_first_login): (bool, bool, bool, bool, bool) = sqlx::query_as(
            "SELECT u.is_staff, u.is_active, u.can_view_all_payroll, u.can_view_all_expenses, p.is_first_login \
             FROM s_user u JOIN s_user_profile p ON u.id = p.user_id"
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch user attributes");

        assert!(is_staff);
        assert!(is_active);
        assert!(can_payroll);
        assert!(can_expenses);
        assert!(!is_first_login);

        cleanup_users(&pool).await.expect("Failed to cleanup");
    }

    #[tokio::test]
    #[ignore = "requires database (TEST_DATABASE_URL)"]
    async fn test_db_idempotent() {
        let _lock = DB_TEST_LOCK.lock().await;

        let pool = create_test_pool().await.expect("Failed to create test pool");
        cleanup_users(&pool).await.expect("Failed to cleanup");

        let input = BootstrapInput {
            email: "admin@test.example.com".to_string(),
            username: "TestAdmin".to_string(),
            password: "TestPass123456".to_string(),
        };

        // 1回目
        let result1 = create_first_admin(&pool, &input).await.expect("Failed first create");
        assert_eq!(result1, Outcome::Created);

        // 2回目
        let result2 = create_first_admin(&pool, &input).await.expect("Failed second create");
        assert_eq!(result2, Outcome::SkippedUsersExist);

        // 件数は1のまま
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user")
            .fetch_one(&pool)
            .await
            .expect("Failed to count users");
        assert_eq!(user_count, 1);

        cleanup_users(&pool).await.expect("Failed to cleanup");
    }

    #[tokio::test]
    #[ignore = "requires database (TEST_DATABASE_URL)"]
    async fn test_db_existing_regular_user_blocks_creation() {
        let _lock = DB_TEST_LOCK.lock().await;

        let pool = create_test_pool().await.expect("Failed to create test pool");
        cleanup_users(&pool).await.expect("Failed to cleanup");

        // 一般ユーザー(管理者ではない)が既にいる状態
        sqlx::query("INSERT INTO s_user (email, password, username, is_staff) VALUES ($1, $2, $3, FALSE)")
            .bind("existing@test.example.com")
            .bind("not-a-real-hash")
            .bind("Existing")
            .execute(&pool)
            .await
            .expect("Failed to insert existing user");

        let input = BootstrapInput {
            email: "admin@test.example.com".to_string(),
            username: "TestAdmin".to_string(),
            password: "TestPass123456".to_string(),
        };
        let result = create_first_admin(&pool, &input).await.expect("create_first_admin failed");
        assert_eq!(result, Outcome::SkippedUsersExist);

        let (count, staff_count): (i64, i64) =
            sqlx::query_as("SELECT COUNT(*), COUNT(*) FILTER (WHERE is_staff) FROM s_user")
                .fetch_one(&pool)
                .await
                .expect("Failed to count users");
        assert_eq!(count, 1);
        assert_eq!(staff_count, 0);

        cleanup_users(&pool).await.expect("Failed to cleanup");
    }

    #[tokio::test]
    #[ignore = "requires database (TEST_DATABASE_URL)"]
    async fn test_db_concurrent() {
        let _lock = DB_TEST_LOCK.lock().await;

        let pool = create_test_pool().await.expect("Failed to create test pool");
        cleanup_users(&pool).await.expect("Failed to cleanup");

        let input = BootstrapInput {
            email: "admin@test.example.com".to_string(),
            username: "TestAdmin".to_string(),
            password: "TestPass123456".to_string(),
        };

        // 2つのタスクで同時に create_first_admin を呼ぶ
        let pool1 = pool.clone();
        let pool2 = pool.clone();
        let input1 = input.clone();
        let input2 = input.clone();

        let (result1, result2) = tokio::join!(
            create_first_admin(&pool1, &input1),
            create_first_admin(&pool2, &input2),
        );

        // どちらかは Created、もう片方は SkippedUsersExist のはず
        let r1 = result1.expect("Failed first concurrent create");
        let r2 = result2.expect("Failed second concurrent create");

        match (r1, r2) {
            (Outcome::Created, Outcome::SkippedUsersExist) |
            (Outcome::SkippedUsersExist, Outcome::Created) => {
                // Expected
            }
            _ => panic!("Unexpected outcome: {:?}, {:?}", r1, r2),
        }

        // 件数は1のまま
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user")
            .fetch_one(&pool)
            .await
            .expect("Failed to count users");
        assert_eq!(user_count, 1);

        cleanup_users(&pool).await.expect("Failed to cleanup");
    }

    async fn create_test_pool() -> Result<sqlx::PgPool, String> {
        let test_db_url = std::env::var("TEST_DATABASE_URL")
            .map_err(|_| "TEST_DATABASE_URL is not set".to_string())?;

        let pool = sqlx::PgPool::connect(&test_db_url)
            .await
            .map_err(|e| format!("Failed to connect to test database: {}", e))?;

        Ok(pool)
    }
}
