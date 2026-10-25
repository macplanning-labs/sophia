#![allow(dead_code)]

mod custom;
mod config;
mod domain;
mod infrastructure;
mod presentation;
mod routes;
mod openapi;



use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use sqlx::migrate::Migrate;

/// Sophia — SES受発注管理システム
#[derive(Parser)]
#[command(name = "sophia", about = "SES受発注管理システム")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Webサーバーを起動する（デフォルト）
    Serve,

    /// EDI DB からデータを同期する
    Sync {
        /// 全件再同期（差分ではなく全件取得）
        #[arg(long, default_value_t = false)]
        full: bool,

        /// ドライラン（DBに書き込まない）
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// EDI DB接続文字列（未指定時は EDI_DATABASE_URL 環境変数）
        #[arg(long)]
        edi_db: Option<String>,
    },

    /// PayrollSystem からデータを移行する
    Migrate {
        /// 移行元DB接続文字列
        #[arg(long)]
        source_db: String,
    },

    /// 移行結果を検証する
    Verify {
        /// 検証対象の移行元DB接続文字列
        #[arg(long)]
        source_db: String,
    },

    /// IMAPメール取得を手動実行する
    FetchEmails,


    /// リマインドメールを送信する（注文書承諾・稼働報告・請求書承諾）
    SendReminders {
        /// ドライラン（送信せず対象を表示）
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// 期限超過チェック（毎朝チェック）を手動実行する（管理者へのメール通知・Google Chat投稿）
    CheckOverdueTasks {
        /// ドライラン（送信せず警告一覧をログ表示）
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// テストメールを送信する（メール設定の動作確認用）
    TestEmail {
        /// 送信先メールアドレス
        #[arg(long)]
        to: String,

        /// テストするテンプレートコード（省略時は全テンプレート一覧を表示）
        #[arg(long)]
        template: Option<String>,
    },

    /// 最初の管理者を作成する(ユーザーが0人のときのみ)
    CreateAdmin {
        /// メールアドレス
        #[arg(long)]
        email: String,

        /// ユーザー名（省略時はメール の @ より前）
        #[arg(long)]
        username: Option<String>,

        /// パスワードを標準入力から読む（指定なければ INITIAL_ADMIN_PASSWORD 環境変数）
        #[arg(long, default_value_t = false)]
        password_stdin: bool,
    },

    /// スキーマ適用済みの既存環境に対して _sqlx_migrations をベースライン（追跡開始）
    BaselineMigrations,

    /// 外部連携の秘密の設定を、今の鍵で暗号化し直す（鍵の入れ替え。INTEGRATION_KEYS_FILE の current の鍵を使う）
    /// 手順: 鍵ファイルに新しい鍵を足して current を変える → このコマンド → 古い鍵を消す
    RekeyIntegrations,

    /// 外部連携を点検する（integrations.toml と実際の設定を照らし合わせる。DB にも相手にも書き込まない）
    /// 必須の連携が使えなければ終了コード 1。最後に `PROBLEM_USES=<用途,…>` の行を出す（デプロイ前の点検が読む）
    CheckIntegrations,

    /// 毎朝 8:45 の外部連携の点検を、いま1回実行する（結果を記録し、前回から変わっていれば Chat とメールで知らせる）
    /// 通常はスケジューラが実行する。ステージングでの確認や、点検のやり直しに使う
    DailyCheckIntegrations,

    /// 口座番号（自社・パートナー・社員の 3 列）の平文を、暗号化して書き戻す（DEMO-000233 S2-G1）
    /// 既定は確認だけ（件数のみ・書き込みなし）。実行は `--execute --i-have-a-backup`（本番のバックアップを取ってから）
    /// 鍵は DATA_KEYS_FILE から読む。1 件でも検証に合わなければ、全体を取り消す。値は出さない。
    EncryptBankAccounts {
        /// 実行する（書き込む）。付けなければ確認だけ
        #[arg(long, conflicts_with = "dry_run")]
        execute: bool,
        /// 確認だけ（既定。明示用）
        #[arg(long)]
        dry_run: bool,
        /// バックアップを取って戻せることを確認した（--execute のときに必須）
        #[arg(long)]
        i_have_a_backup: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_file(true)
            .with_line_number(true))
        .init();

    // 環境変数読み込み: ENV_NAME に応じて .env.{ENV_NAME} を読み込む
    // デフォルトは local → .env.local
    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "local".to_string());
    let env_file = format!(".env.{}", env_name);
    if dotenvy::from_filename(&env_file).is_ok() {
        tracing::info!("📁 環境変数ファイル読み込み: {}", env_file);
    } else {
        tracing::warn!("⚠️ 環境変数ファイルが見つかりません: {}", env_file);
    }

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Serve => {
            serve().await?;
        }
        Commands::Sync { full, dry_run, edi_db } => {
            tracing::info!("🔄 EDI→Sophia データ同期開始");

            let sophia_pool = infrastructure::db::create_pool().await?;
            run_migrations(&sophia_pool).await?;

            let edi_url = edi_db
                .or_else(|| std::env::var("EDI_DATABASE_URL").ok())
                .ok_or_else(|| anyhow::anyhow!("EDI_DATABASE_URL が設定されていません。--edi-db または環境変数で指定してください"))?;

            let edi_pool = sqlx::PgPool::connect(&edi_url).await?;
            tracing::info!("  EDI DB接続成功");

            let mut sync = infrastructure::edi_sync::EdiSync::new(sophia_pool, edi_pool);
            sync.run(full, dry_run).await?;

            tracing::info!("✅ EDI同期完了");
        }
        Commands::Migrate { source_db } => {
            tracing::info!("📦 PayrollSystem データ移行開始");
            tracing::info!("   Source: {}", source_db);

            // 移行先DB接続
            let target_pool = infrastructure::db::create_pool().await?;
            run_migrations(&target_pool).await?;

            // 移行元DB接続
            let _source_pool = sqlx::PgPool::connect(&source_db).await?;

            // 社員データ移行
            tracing::info!("📋 社員データ移行中...");
            let rows = sqlx::query(
                r#"
                INSERT INTO m_employee (employee_id, last_name, first_name, last_name_kana, first_name_kana,
                    employment_type, birth_date, hire_date, email, base_salary, position_allowance,
                    housing_allowance, commuting_allowance, standard_monthly_hours, standard_remuneration,
                    dependents_count, is_active)
                SELECT
                    e.employee_id, e.last_name, e.first_name,
                    COALESCE(e.last_name_kana, ''), COALESCE(e.first_name_kana, ''),
                    COALESCE(e.employment_type, 'REGULAR'),
                    e.birth_date, e.hire_date, COALESCE(e.email, ''),
                    COALESCE(e.base_salary, 0), COALESCE(e.position_allowance, 0),
                    COALESCE(e.housing_allowance, 0), COALESCE(e.commuting_allowance, 0),
                    COALESCE(e.standard_monthly_hours, 180.0), COALESCE(e.standard_remuneration, 0),
                    COALESCE(e.dependents_count, 0), COALESCE(e.is_active, true)
                FROM dblink($1, 'SELECT * FROM payroll_employee') AS e(
                    id int, employee_id varchar, last_name varchar, first_name varchar,
                    last_name_kana varchar, first_name_kana varchar, employment_type varchar,
                    birth_date date, hire_date date, email varchar,
                    base_salary int, position_allowance int, housing_allowance int, commuting_allowance int,
                    standard_monthly_hours numeric, standard_remuneration int,
                    dependents_count int, is_active boolean,
                    created_at timestamptz, updated_at timestamptz
                )
                ON CONFLICT (employee_id) DO NOTHING
                "#
            )
            .bind(&source_db)
            .execute(&target_pool)
            .await;

            match rows {
                Ok(r) => tracing::info!("  ✅ 社員: {} 件移行", r.rows_affected()),
                Err(e) => {
                    tracing::warn!("  ⚠️ 社員移行エラー（dblink未設定の場合は正常）: {}", e);
                    tracing::info!("  → 手動CSV移行を検討してください");
                }
            }

            tracing::info!("✅ データ移行完了");
        }
        Commands::Verify { source_db } => {
            tracing::info!("🔍 移行検証開始: {}", source_db);

            let target_pool = infrastructure::db::create_pool().await?;

            // 各テーブルのカウントを比較
            let tables = ["m_employee", "m_client", "m_partner", "m_project"];
            for table in tables {
                let count: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {}", table))
                    .fetch_one(&target_pool)
                    .await?;
                tracing::info!("  {} : {} 件", table, count.0);
            }

            tracing::info!("✅ 検証完了");
        }

        Commands::FetchEmails => {
            tracing::info!("📨 メール自動取込パイプラインを手動実行します...");

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            let result = infrastructure::mail_pipeline::run_pipeline(&pool).await;
            tracing::info!("✅ パイプライン完了: {result}");

            if result.has_errors() {
                tracing::error!("  ❌ エラーが発生しました。詳細はログを確認してください");
            }
        }

        Commands::SendReminders { dry_run } => {
            tracing::info!("📧 リマインドメール送信{}", if dry_run { "（ドライラン）" } else { "" });

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            domain::services::reminder_service::send_all_reminders(&pool, dry_run).await?;
        }

        Commands::CheckOverdueTasks { dry_run } => {
            tracing::info!("⏰ 期限超過チェック（毎朝チェック）{}", if dry_run { "（ドライラン）" } else { "" });

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            domain::services::reminder_service::check_overdue_tasks(&pool, dry_run).await?;
        }

        Commands::TestEmail { to, template } => {
            tracing::info!("🧪 テストメール送信: {} → {:?}", to, template);

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            domain::services::email_test::run_email_test(&pool, &to, template.as_deref()).await?;
        }

        Commands::CreateAdmin { email, username, password_stdin } => {
            tracing::info!("👤 初期管理者作成");

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            match infrastructure::bootstrap_admin::run_cli(&pool, &email, username.as_deref(), password_stdin).await {
                Ok(true) => {}
                Ok(false) => std::process::exit(1),
                Err(e) => return Err(e),
            }
        }

        Commands::BaselineMigrations => {
            baseline_migrations().await?;
        }

        Commands::RekeyIntegrations => {
            let pool = infrastructure::db::create_pool().await?;
            let cipher = infrastructure::integrations::secret::SecretCipher::from_env().map_err(|e| anyhow::anyhow!(e))?;
            let rows = infrastructure::repositories::integration_repo::list_all_rows(&pool).await?;
            let (mut done, mut skipped) = (0usize, 0usize);
            for r in rows {
                let (Some(enc), Some(nonce), Some(kid)) = (&r.secret_enc, &r.secret_nonce, &r.secret_key_id) else { continue };
                if kid == cipher.current_key_id() {
                    skipped += 1;
                    continue;
                }
                let plain = cipher.open(kid, nonce, enc).map_err(|e| anyhow::anyhow!("「{}」(id {}): {e}", r.label, r.id))?;
                let sealed = cipher.seal(&plain).map_err(|e| anyhow::anyhow!(e))?;
                infrastructure::repositories::integration_repo::replace_secret(
                    &pool,
                    r.id,
                    infrastructure::repositories::integration_repo::SealedSecret { key_id: &sealed.key_id, nonce: &sealed.nonce, ciphertext: &sealed.ciphertext },
                )
                .await?;
                done += 1;
            }
            println!("暗号化し直した: {done} 件 / すでに今の鍵: {skipped} 件(今の鍵: {})", cipher.current_key_id());
        }

        Commands::EncryptBankAccounts { execute, dry_run: _, i_have_a_backup } => {
            use infrastructure::bank_account_encryption as enc;
            use infrastructure::data_crypto::DataCrypto;

            if execute {
                // 実行は、バックアップの確認と、実行のゲート(環境 + アプリが暗号文を読めるか)が済んでからだけ。DB に触る前に止める
                if !i_have_a_backup {
                    anyhow::bail!(
                        "実行には、バックアップを取って戻せることを確認してから --i-have-a-backup を付けてください。\
                         （確認だけなら --dry-run。何も書き込まれていません）"
                    );
                }
                let dc = DataCrypto::from_env().map_err(|e| anyhow::anyhow!(e))?;
                enc::execute_gate(&dc).map_err(|e| anyhow::anyhow!(e))?;

                let pool = infrastructure::db::create_pool().await?;
                // 口座番号の列を TEXT に広げてから変換する（暗号化した値は 20 文字を超える）。実行時だけマイグレーションを流す
                run_migrations(&pool).await?;
                tracing::warn!("口座番号の変換を開始します。変換の間、この表への書き込みは止まります。");
                let converted = enc::convert(&pool, &dc).await?;
                for (t, n) in &converted {
                    println!("{}: 暗号化して書き戻した {} 件", t.label(), n);
                }
                println!("完了しました（検証済み）。もう一度確認すると、平文は 0 件になります。");
            } else {
                // 確認はマイグレーションを流さない(読み取りだけ)。083 が未適用なら、件数は出さずに終わる
                let dc = DataCrypto::from_env().map_err(|e| anyhow::anyhow!(e))?;
                let pool = infrastructure::db::create_pool().await?;
                let mut conn = pool.acquire().await?;
                let text_ok = infrastructure::repositories::bank_account_repo::all_text(&mut conn).await?;
                drop(conn);
                if !text_ok {
                    println!("083 未適用（口座番号の列が TEXT に広がっていません）。確認は行いませんでした。");
                    println!("アプリを起動してマイグレーション 083 を適用してから、もう一度確認してください。");
                } else {
                    let counts = enc::scan(&pool, &dc).await?;
                    println!("【確認のみ（書き込みなし。値は表示しない）】");
                    for c in &counts {
                        println!(
                            "{}: 全 {} 件 / 暗号化済み {} 件（うち今の鍵以外 {} 件、復号できない {} 件） / 平文 {} 件（うち空欄 {} 件）",
                            c.target.label(), c.total, c.encrypted, c.other_key, c.undecryptable, c.plaintext, c.plaintext_empty
                        );
                    }
                    let undecryptable: usize = counts.iter().map(|c| c.undecryptable).sum();
                    if undecryptable > 0 {
                        println!("復号できない行が {undecryptable} 件あります。この件数が 0 になるまで、実行できません（鍵・鍵IDを確認してください）。");
                    }
                    match enc::execute_gate(&dc) {
                        Ok(()) => println!("実行するには、本番のバックアップを取ってから: --execute --i-have-a-backup"),
                        Err(e) => println!("現在の設定では実行できません: {e}"),
                    }
                }
            }
        }

        Commands::CheckIntegrations => {
            // 点検は読み取りだけ（マイグレーションも流さない）
            let pool = infrastructure::db::create_pool().await?;
            infrastructure::integrations::reload(&pool).await;
            let reports = infrastructure::integrations::check::run(&pool, false).await;
            if infrastructure::integrations::check::print_report(&reports) {
                std::process::exit(1);
            }
        }

        Commands::DailyCheckIntegrations => {
            let pool = infrastructure::db::create_pool().await?;
            infrastructure::integrations::check::run_daily(pool).await;
        }
    }

    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    // 設定読み込み
    let config = config::AppConfig::from_env()?;

    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "local".to_string());

    // 信用する中継(TRUSTED_PROXY_CIDRS)を整える（DEMO-000233 S2-C）。
    // 未設定・空のとき、本番系(production/dr/staging)は起動を止め、それ以外は none にそろえる。
    // 送り元を決める2つの読み手(Sophia と auth-core の試行ロック)が、同じ値を読むようにするため。
    // set_var は HTTP サーバーの起動より前、送り元の判定(OnceLock で初めて読む)より前に呼ぶ。読む側はまだ動いていない。
    let trusted_cidrs = std::env::var(auth_core::infrastructure::client_ip::TRUSTED_PROXY_CIDRS_ENV).ok();
    match presentation::auth_messages::trusted_proxy_fallback(&env_name, trusted_cidrs.as_deref()) {
        Ok(Some(value)) => {
            tracing::warn!(
                "⚠️ TRUSTED_PROXY_CIDRS が未設定です（ENV_NAME={}）。none（何も信用しない）として起動します。nginx の範囲を設定してください",
                env_name
            );
            std::env::set_var(auth_core::infrastructure::client_ip::TRUSTED_PROXY_CIDRS_ENV, value);
        }
        Ok(None) => {}
        Err(message) => anyhow::bail!(message),
    }
    // 権限判定(AUTHZ_MODE)が強制(enforce)でなければ、本番系は起動を止める(DEMO-000254)。
    // 未設定の影モードでは、権限の無い社員が他人の請求書などを読めてしまう。local など他の環境は何もしない。
    presentation::auth_messages::authz_enforce_startup_check(
        &env_name,
        std::env::var("AUTHZ_MODE").ok().as_deref(),
        std::env::var("AUTHZ_ENFORCE_DOMAINS").ok().as_deref(),
    )
    .map_err(|message| anyhow::anyhow!(message))?;
    // 口座番号などの列の暗号化の設定(DEMO-000233 S2-G1)。本番・DR・ステージングで、有効なのに鍵が読めなければ起動を止める
    let data_crypto = infrastructure::data_crypto::DataCrypto::from_env().map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!("データ暗号化: 書き込みの暗号化 {} / 平文の読み取り {}", if data_crypto.enabled() { "有効" } else { "無効(移行前)" }, if data_crypto.required() { "拒否" } else { "許可" });
    // リポジトリ層が使うプロセス共有の設定(S2-G2-1)。二度目の初期化は起こらない
    infrastructure::data_crypto::install(data_crypto).map_err(|e| anyhow::anyhow!(e))?;

    // WebAuthn allowlist の設定漏れを早期検知する（本番/DRで未設定だとWebAuthnが
    // 常にlocalhostのみ許可され、実ドメインからのパスキー登録/認証が必ず失敗するため）
    if matches!(env_name.as_str(), "production" | "dr" | "staging")
        && std::env::var("WEBAUTHN_ALLOWED_HOSTS")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
    {
        tracing::warn!(
            "⚠️ WEBAUTHN_ALLOWED_HOSTS が未設定です（ENV_NAME={}）。localhost以外のHostからの \
             WebAuthn登録/認証は全て拒否されます。実ドメインを設定してください",
            env_name
        );
    }

    // DB接続
    let pool = infrastructure::db::create_pool().await?;

    // マイグレーション実行（SQLファイルを順序付きで実行）
    run_migrations(&pool).await?;

    // 口座番号の暗号化の自己点検(DEMO-000233 S2-G2-5)。暗号化が有効なときだけ。件数だけをログに出す。
    // 復号できない行は開発以外で起動を止める。平文の行は REQUIRED なら止め、それ以外は警告する
    let crypto_now = infrastructure::data_crypto::global().map_err(|e| anyhow::anyhow!(e))?;
    if crypto_now.enabled() {
        let strict = !infrastructure::data_crypto::is_dev_env(&env_name);
        infrastructure::bank_account_encryption::startup_check(&pool, crypto_now, strict).await?;
    }

    // 初期管理者を作成（環境変数で指定されていれば）
    infrastructure::bootstrap_admin::run_from_env(&pool).await;

    // 外部連携の点検（integrations.toml）。止めずに、結果をログに残し、欠けがあれば知らせる（DEMO-000213）。
    // Drive などへの問い合わせで起動を待たせないよう、裏で実行する。
    // 画面の設定(DB)を読み込んでから点検する(DEMO-000215)
    infrastructure::integrations::reload(&pool).await;
    tokio::spawn(infrastructure::integrations::check::run_at_startup(pool.clone()));

    // AppState 構築
    // WebAuthn の RP ID/Origin は固定値を持たず、リクエストごとに Host ヘッダーから
    // 解決する（webauthn_service::create_webauthn_from_headers）
    // 試行の数えは DB(s_login_attempt)に保存する(DEMO-000233 S2-D)。再起動・複数台をまたいでも残る
    let portal_attempt_store = std::sync::Arc::new(
        infrastructure::repositories::attempt_lock_store::PgAttemptStore::new(
            pool.clone(),
            infrastructure::repositories::login_attempt_repo::KIND_PORTAL,
            auth_core::domain::attempt_lock::AttemptLockConfig::default(),
        ),
    );
    let mfa_attempt_store = std::sync::Arc::new(
        infrastructure::repositories::attempt_lock_store::PgAttemptStore::new(
            pool.clone(),
            infrastructure::repositories::login_attempt_repo::KIND_MFA,
            auth_core::domain::attempt_lock::AttemptLockConfig::default(),
        ),
    );
    let passkey_attempt_store = std::sync::Arc::new(
        infrastructure::repositories::attempt_lock_store::PgAttemptStore::new(
            pool.clone(),
            infrastructure::repositories::login_attempt_repo::KIND_PASSKEY,
            auth_core::domain::attempt_lock::AttemptLockConfig::default(),
        ),
    );
    let passkey_begin_attempt_store = std::sync::Arc::new(
        infrastructure::repositories::attempt_lock_store::PgAttemptStore::new(
            pool.clone(),
            infrastructure::repositories::login_attempt_repo::KIND_PASSKEY_BEGIN,
            auth_core::domain::attempt_lock::AttemptLockConfig {
                max_attempts: infrastructure::repositories::login_attempt_repo::PASSKEY_BEGIN_MAX,
                ..auth_core::domain::attempt_lock::AttemptLockConfig::default()
            },
        ),
    );
    let state = config::AppState {
        pool,
        secret_key: config.secret_key.clone(),
        portal_attempt_store,
        mfa_attempt_store,
        passkey_attempt_store,
        passkey_begin_attempt_store,
        // 社内 SSO(SSO_ENABLED=true のときだけ)。設定が足りなければ、AppConfig::from_env で起動が止まっている
        sso: match &config.sso {
            Some(s) => {
                tracing::info!("社内 SSO を有効にしました({})", s.cf.team_domain);
                Some(std::sync::Arc::new(config::SsoRuntime::new(s)?))
            }
            None => None,
        },
    };

    // 二要素認証の試行の記録のうち、期限切れの行を消す(DEMO-000153 S-1)。5 分ごとに、古い順に 1000 件ずつ(最大 10 回まで続けて)。
    // 最初の実行は 5 分後(起動の直後は、マイグレーションの適用が終わる前で、テーブルがまだ無いことがある)
    let purge_pool = state.pool.clone();
    tokio::spawn(async move {
        let period = std::time::Duration::from_secs(300);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        let window = chrono::Duration::minutes(
            infrastructure::repositories::mfa_attempt_repo::MFA_LOCK_WINDOW_MINUTES,
        );
        loop {
            interval.tick().await;
            for _ in 0..10 {
                match infrastructure::repositories::mfa_attempt_repo::purge_expired(&purge_pool, window, 1000).await {
                    Ok(n) if n >= 1000 => continue,
                    Ok(_) => break,
                    Err(e) => {
                        tracing::error!("二要素認証の試行記録の掃除に失敗: {:?}", e);
                        break;
                    }
                }
            }
            // ログインの試行の記録(7日を過ぎた行)も、同じ周期で消す(DEMO-000233 S2-D)
            if let Err(e) = infrastructure::repositories::login_attempt_repo::purge_expired(&purge_pool, 5000).await {
                tracing::error!("ログイン試行の記録の掃除に失敗: {:?}", e);
            }
            // トークンURLの利用の監査(token_access)のうち 90 日を過ぎた行も、古い順に 5000 件まで消す(DEMO-000233 S2-E)。
            // 失敗しても本体は止めない(他の種別の行は消さない)
            if let Err(e) = infrastructure::repositories::auth_repo::purge_expired_token_access(
                &purge_pool,
                presentation::middleware::token_guard::EVENT_TOKEN_ACCESS,
                5000,
            )
            .await
            {
                tracing::error!("トークンの利用の監査の掃除に失敗: {:?}", e);
            }
        }
    });

    // メール取得スケジューラをバックグラウンドで起動（毎時実行）
    let scheduler_pool = state.pool.clone();
    tokio::spawn(async move {
        infrastructure::scheduler::start_scheduler(scheduler_pool).await;
    });

    // ルーター構築
    let app = routes::create_router(state);

    // サーバー起動
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("🚀 Sophia server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // /v1/ WebAPIのIPベースレート制限（PeerIpKeyExtractor）が ConnectInfo<SocketAddr> を
    // 要求するため、into_make_service_with_connect_info が必須（プレーンな app だと
    // 常に GovernorError::UnableToExtractKey で500になる）。
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}


/// migrations/ を sqlx 標準 Migrator で適用する（追跡テーブルは `_sqlx_migrations`）。
async fn run_migrations(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    // 旧ファイル名（043_paid_leave.sql）を新名へ（既存 s_migration 行の互換）。
    // テーブルが無い新規環境では失敗しうるので無視して続行する。
    if let Err(e) = sqlx::query(
        "UPDATE s_migration SET filename = '054_paid_leave.sql' WHERE filename = '043_paid_leave.sql'"
    )
    .execute(pool)
    .await
    {
        tracing::debug!("s_migration rename skipped (table may not exist): {}", e);
    }

    // sqlx 標準 Migrator を使用
    let migrator = sqlx::migrate!("./migrations");
    // 公開版で書き換えたマイグレーション（旧版を適用済みの DB）のチェックサムを読み替える。社内版では何もしない
    infrastructure::migration_compat::accept_rewritten(pool, &migrator).await
        .map_err(|e| anyhow::anyhow!("マイグレーションのチェックサムの読み替えに失敗: {e}"))?;
    migrator.run(pool).await
        .map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;

    tracing::info!("✅ All migrations applied");
    Ok(())
}

/// スキーマ適用済みの既存環境に対して `_sqlx_migrations` をベースラインする。
///
/// checksum / description / version は `sqlx::migrate!` が解決した値をそのまま使う
///（sqlx 本体は SQL 本文の SHA-384。独自ハッシュだと起動時に不一致で失敗する）。
async fn baseline_migrations() -> anyhow::Result<()> {
    let pool = infrastructure::db::create_pool().await?;
    let migrator = sqlx::migrate!("./migrations");

    let mut conn = pool.acquire().await?;
    conn.ensure_migrations_table()
        .await
        .map_err(|e| anyhow::anyhow!("_sqlx_migrations 準備失敗: {e}"))?;

    let applied = conn
        .list_applied_migrations()
        .await
        .map_err(|e| anyhow::anyhow!("適用済み一覧の取得失敗: {e}"))?;

    tracing::info!("📊 _sqlx_migrations の現在の行数: {}", applied.len());

    if !applied.is_empty() {
        return Err(anyhow::anyhow!(
            "❌ _sqlx_migrations が既に埋まっています（{} 行）。ベースラインは不要です",
            applied.len()
        ));
    }

    // スキーマ適用確認（代表テーブル存在確認）
    let schema_applied: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 't_purchase_order')",
    )
    .fetch_one(&mut *conn)
    .await?;

    if !schema_applied {
        return Err(anyhow::anyhow!(
            "❌ スキーマが適用されていません。空DBならアプリ起動（migrate run）を先に実行してください"
        ));
    }

    tracing::info!("✅ スキーマ確認OK（t_purchase_order 存在）");

    let mut baseline_count = 0;
    for migration in migrator.iter() {
        // reversible down は適用記録対象外（sqlx run と同様）
        if !migration.migration_type.is_up_migration() {
            continue;
        }

        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
             VALUES ($1, $2, NOW(), true, $3, 0)",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(&*migration.checksum)
        .execute(&mut *conn)
        .await?;

        tracing::debug!(
            "  ✅ version={} description={}",
            migration.version,
            migration.description
        );
        baseline_count += 1;
    }

    if baseline_count == 0 {
        return Err(anyhow::anyhow!("❌ 記録対象のマイグレーションがありません"));
    }

    tracing::info!("✅ {} 個のマイグレーションをベースライン記録しました", baseline_count);
    tracing::info!("💡 次回起動時、起動時マイグレーションは全て「適用済み」として扱われます");

    Ok(())
}
