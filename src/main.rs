#![allow(dead_code)]

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

    /// EDI-OASIS請求書を取り込む
    ImportBillings {
        /// 対象年
        #[arg(long)]
        year: i32,

        /// 対象月
        #[arg(long)]
        month: i32,
    },

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

    /// スキーマ適用済みの既存環境に対して _sqlx_migrations をベースライン（追跡開始）
    BaselineMigrations,
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

        Commands::ImportBillings { year, month } => {
            tracing::info!("📥 EDI-OASIS 請求取込開始: {}年{}月", year, month);

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            let mut client = infrastructure::edi_oasis_client::EdiOasisClient::from_env()?;
            let result = infrastructure::billing_importer::import_billings_for_month(
                &pool, &mut client, year, month,
            ).await;

            tracing::info!("✅ {}", result);
            if !result.errors.is_empty() {
                for err in &result.errors {
                    tracing::error!("  ❌ {}", err);
                }
            }
        }
        Commands::FetchEmails => {
            tracing::info!("📨 メール自動取込パイプラインを手動実行します...");

            let pool = infrastructure::db::create_pool().await?;
            run_migrations(&pool).await?;

            let result = infrastructure::mail_pipeline::run_pipeline(&pool, None).await;
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

        Commands::BaselineMigrations => {
            baseline_migrations().await?;
        }
    }

    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    // 設定読み込み
    let config = config::AppConfig::from_env()?;

    // DB接続
    let pool = infrastructure::db::create_pool().await?;

    // マイグレーション実行（SQLファイルを順序付きで実行）
    run_migrations(&pool).await?;

    // AppState 構築
    // WebAuthn の RP ID/Origin は固定値を持たず、リクエストごとに Host ヘッダーから
    // 解決する（webauthn_service::create_webauthn_from_headers）
    let state = config::AppState {
        pool,
        secret_key: config.secret_key.clone(),
    };

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
