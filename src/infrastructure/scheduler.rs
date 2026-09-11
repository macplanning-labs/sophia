/// infrastructure/scheduler.rs — 定期実行スケジューラ
///
/// EDI_MP の tasks/scheduler.py (APScheduler) をRustに移植。
/// tokio-cron-scheduler でバックグラウンドジョブを管理する。
///
/// ## 登録ジョブ
/// - メール自動取込パイプライン: 15分毎（Phase1〜4を`mail_pipeline::run_pipeline`で一括実行）
///   旧「メール取得(毎時0分)」「EDI注文書取込(毎時5分)」の2ジョブを統合したもの。

use sqlx::PgPool;
use tokio_cron_scheduler::{Job, JobScheduler};

/// スケジューラを起動し、定期ジョブを登録する
///
/// axum サーバー起動時に tokio::spawn で呼び出す。
pub async fn start_scheduler(pool: PgPool) {
    let scheduler = match JobScheduler::new().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("[スケジューラ] 初期化エラー: {e}");
            return;
        }
    };

    // ── ジョブ1: メール自動取込パイプライン（15分毎） ──
    // Phase1(監視・分類)→Phase2(データソース取得)→Phase3(解析・登録)→Phase4(保存・通知)を
    // run_pipeline() が順に呼ぶ。ダッシュボードの手動トリガーAPIも同じ関数を呼ぶため、
    // 自動/手動でロジックが分岐・重複することはない。
    //
    // MAIL_PIPELINE_ENABLED=false でこの自動ジョブのみ無効化できる（手動トリガーAPIは影響を受けない）。
    // ステージング環境でGmailの実アカウントに定期的にIMAPログインする必要がない場合に使用する。
    let mail_pipeline_enabled = std::env::var("MAIL_PIPELINE_ENABLED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    if !mail_pipeline_enabled {
        tracing::info!("[スケジューラ] メール自動取込パイプライン: MAIL_PIPELINE_ENABLED=false のため登録スキップ");
    } else {
        let pool_clone = pool.clone();
        let pipeline_job = Job::new_async("0 */15 * * * *", move |_uuid, _lock| {
            let pool = pool_clone.clone();
            Box::pin(async move {
                tracing::info!("[スケジューラ] メール自動取込パイプライン開始");
                let result = crate::infrastructure::mail_pipeline::run_pipeline(&pool, None).await;
                tracing::info!("[スケジューラ] メール自動取込パイプライン完了: {result}");
            })
        });

        match pipeline_job {
            Ok(job) => {
                if let Err(e) = scheduler.add(job).await {
                    tracing::error!("[スケジューラ] メール自動取込パイプラインジョブ登録エラー: {e}");
                    return;
                }
                tracing::info!("[スケジューラ] メール自動取込パイプラインジョブ登録完了（15分毎）");
            }
            Err(e) => {
                tracing::error!("[スケジューラ] メール自動取込パイプラインジョブ作成エラー: {e}");
                return;
            }
        }
    }

    // ── ジョブ2: リマインド一括送信（毎日 0:00 UTC = JST 9:00） ──
    // EDI互換: _approval_reminder_job, _report_reminder_job, _deadline_reminder_job, _overdue_check_job
    let pool_clone3 = pool.clone();
    let reminder_job = Job::new_async("0 0 0 * * *", move |_uuid, _lock| {
        let pool = pool_clone3.clone();
        Box::pin(async move {
            tracing::info!("[スケジューラ] リマインド一括送信ジョブ開始");
            // 1. 注文書承諾リマインド + 稼働報告リマインド + 請求書承諾リマインド
            if let Err(e) = crate::domain::services::reminder_service::send_all_reminders(&pool, false).await {
                tracing::error!("[スケジューラ] リマインド送信エラー: {e}");
            }
            // 2. 期限超過チェック（管理者通知）
            if let Err(e) = crate::domain::services::reminder_service::check_overdue_tasks(&pool, false).await {
                tracing::error!("[スケジューラ] 期限超過チェックエラー: {e}");
            }
            tracing::info!("[スケジューラ] リマインド一括送信ジョブ完了");
        })
    });

    match reminder_job {
        Ok(job) => {
            if let Err(e) = scheduler.add(job).await {
                tracing::error!("[スケジューラ] リマインドジョブ登録エラー: {e}");
            } else {
                tracing::info!("[スケジューラ] リマインドジョブ登録完了（毎日JST 9:00）");
            }
        }
        Err(e) => {
            tracing::error!("[スケジューラ] リマインドジョブ作成エラー: {e}");
        }
    }

    // ── ジョブ3: JWT ブラックリスト掃除（毎日 00:30 UTC = JST 9:30） ──
    let pool_clone_purge = pool.clone();
    let purge_job = Job::new_async("0 30 0 * * *", move |_uuid, _lock| {
        let pool = pool_clone_purge.clone();
        Box::pin(async move {
            let repo = crate::infrastructure::repositories::jwt_blacklist_repo::JwtBlacklistRepo(pool);
            match repo.purge_expired().await {
                Ok(deleted) => {
                    tracing::info!("[スケジューラ] JWT ブラックリスト掃除完了: {}件削除", deleted);
                }
                Err(_) => {
                    tracing::error!(
                        "[認証/ブラックリスト掃除] 処理=purge_expired 結果=失敗 影響=期限切れjti行が残る（認証判定自体はexp検証で正しい）"
                    );
                }
            }
        })
    });

    match purge_job {
        Ok(job) => {
            if let Err(e) = scheduler.add(job).await {
                tracing::error!("[スケジューラ] JWT ブラックリスト掃除ジョブ登録エラー: {e}");
            } else {
                tracing::info!("[スケジューラ] JWT ブラックリスト掃除ジョブ登録完了（毎日JST 9:30）");
            }
        }
        Err(e) => {
            tracing::error!("[スケジューラ] JWT ブラックリスト掃除ジョブ作成エラー: {e}");
        }
    }

    // スケジューラ起動
    if let Err(e) = scheduler.start().await {
        tracing::error!("[スケジューラ] 起動エラー: {e}");
        return;
    }

    tracing::info!("[スケジューラ] 起動完了");

    // スケジューラはバックグラウンドで動作し続ける
    // start() は内部で tokio::spawn しているため、ここでブロックしなくてよい
    // ただし scheduler 変数がドロップされないように保持する必要がある
    // → 無限スリープでライフタイムを維持
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}
