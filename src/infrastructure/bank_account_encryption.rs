//! 既存の口座番号(平文)を、暗号化して書き戻す変換(`sophia encrypt-bank-accounts`)。DEMO-000233 項目4 / S2-G1
//!
//! - 確認(dry-run): 対象件数・平文の件数・暗号化済みの件数・復号できない件数だけを出す。値は出さない。DB には書き込まない。
//!   マイグレーションも流さない(未適用なら「083 未適用」と出して終わる)。
//! - 実行(execute): 1 つのトランザクションで、平文の行と旧鍵の行を今の鍵で暗号化して書き戻す。書いた後に読み直して、
//!   復号すると元の値と一致することを確かめてからコミットする。1 件でも合わなければ全体を取り消す。
//!   復号できない行(`enc1:` で始まるが、どの鍵でも開けない行)が 1 件でもあれば、件数を出して止める(DB は変えない)。
//!   実行は、ゲート(`execute_gate`)を通ったときだけ。ゲートは「このビルドが暗号文を読めること(`APP_READS_ENCRYPTED`)」「環境で有効化されていること(ENABLED=true)」「鍵が読めていること」の 3 つを見る。
//! - 起動時の自己点検(`startup_check`): ENABLED=true の serve 起動時に 3 列を全行走査し、件数だけをログに出す。復号できない行は strict 環境で起動を止める。平文の行は、REQUIRED=true なら起動を止め、そうでなければ警告する。
//! - 冪等: 今の鍵で暗号化済み(復号できる・鍵IDが現行)の行は触らない。2 回流しても二重に暗号化しない。
//! - 鍵の入れ替え: 平文に加えて、今の鍵以外で暗号化された行も、今の鍵で暗号化し直す(旧鍵を消す前に流す)。
//! - 値(平文・鍵)は、ログにもエラーにも出さない。件数だけを出す。

use std::collections::HashMap;

use anyhow::{anyhow, bail};
use sqlx::PgPool;

use crate::domain::services::chat_notifier;
use crate::infrastructure::data_crypto::{DataCrypto, APP_READS_ENCRYPTED};
use crate::infrastructure::repositories::bank_account_repo::{self, Target, TARGETS};

/// 表ごとの件数(値は含まない)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Count {
    pub target: Target,
    /// 行数
    pub total: usize,
    /// 暗号化済み(enc1:)の行(復号できないものを含む)
    pub encrypted: usize,
    /// 平文のままの行(変換の対象。空欄を含む)
    pub plaintext: usize,
    /// うち空欄の行(平文の内訳)
    pub plaintext_empty: usize,
    /// 暗号化済みだが、今の鍵以外で暗号化された行(鍵の入れ替えで rekey が要る行)
    pub other_key: usize,
    /// enc1: で始まるが、どの鍵でも復号できない行(実行を止める。値は出さない)
    pub undecryptable: usize,
}

/// 実行のゲートの判定(純粋な関数。試験はこれを直接試す)。3 つとも揃ったときだけ通す。
pub fn gate_rule(env_enabled: bool, app_reads_encrypted: bool, key_loaded: bool) -> Result<(), String> {
    if !app_reads_encrypted {
        return Err("このビルドでは変換の実行を拒否します。暗号化された値を画面・帳票で読めるビルドでだけ実行できます".into());
    }
    if !env_enabled {
        return Err("DATA_ENCRYPTION_ENABLED=true の環境でだけ実行できます（確認だけなら --dry-run）".into());
    }
    if !key_loaded {
        return Err("暗号化の鍵が読めていないため、実行できません(DATA_KEYS_FILE を確認してください)".into());
    }
    Ok(())
}

/// 実行のゲート。設定(`dc`)とこのビルドの設定で判定する。DB には触らない
pub fn execute_gate(dc: &DataCrypto) -> Result<(), String> {
    gate_rule(dc.enabled(), APP_READS_ENCRYPTED, dc.cipher().is_some())
}

/// 起動時の自己点検の結果(件数だけ。値は持たない)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartupCheck {
    /// 3 列の全行数(空欄・平文・暗号文のすべて)
    pub total: usize,
    /// 暗号化済み(enc1:)の行の合計(復号できないものを含む)
    pub encrypted: usize,
    /// 平文の行の合計(空欄を含む)
    pub plaintext: usize,
    /// 平文の行のうち、空欄でないもの(REQUIRED で拒否される対象)
    pub plaintext_nonempty: usize,
    /// 復号できない行の合計
    pub undecryptable: usize,
}

/// 復号できない行があるとき、起動を止めるか(純粋な関数。試験はこれを直接試す)。
///
/// 止める場合は次の 2 つだけ(S2-G2-5 の判断。安全側に寄せた理由は計画書 17 章):
/// - 暗号化済みの行が**全部**復号できない: 鍵が違う・消えている可能性が高く、画面・帳票が広く壊れるため。
/// - 復号できない行が 3 列の全行数の**過半数**: 表そのものが壊れている可能性が高いため。
///
/// それ以外(少数の破損)は起動を続ける。その行だけが読めなくなる(読み出し時に 500)。
/// 少数でも件数は警告ログと運用の Chat(High)に知らせる。値・行の特定は出さない。
pub fn undecryptable_stops_startup(sum: &StartupCheck) -> bool {
    sum.undecryptable > 0 && (sum.undecryptable == sum.encrypted || sum.undecryptable * 2 > sum.total)
}

/// 起動時の自己点検(serve が ENABLED=true のときに呼ぶ)。3 列の全行を走査し、件数だけをログに出す。
/// - 復号できない行があり `strict`(開発以外の環境)のとき: `undecryptable_stops_startup` なら Err(起動を止める)。
///   少数の破損なら起動を続け、警告ログと運用の Chat(High。1 起動 1 回)で知らせる。開発では警告ログだけ。
/// - 平文の行(空欄以外)があり REQUIRED=true なら Err(平文は読めないため)。移行期間中(REQUIRED=false)は警告だけ。
pub async fn startup_check(pool: &PgPool, dc: &DataCrypto, strict: bool) -> anyhow::Result<StartupCheck> {
    let counts = scan(pool, dc).await?;
    let mut sum = StartupCheck::default();
    for c in &counts {
        sum.total += c.total;
        sum.encrypted += c.encrypted;
        sum.plaintext += c.plaintext;
        sum.plaintext_nonempty += c.plaintext - c.plaintext_empty;
        sum.undecryptable += c.undecryptable;
    }
    tracing::info!(
        "[データ暗号化] 起動時の点検: 全 {} 件 / 暗号化済み {} 件 / 平文 {} 件(うち空欄以外 {} 件) / 復号できない {} 件(3 列の合計。値は出しません)",
        sum.total, sum.encrypted, sum.plaintext, sum.plaintext_nonempty, sum.undecryptable
    );
    if sum.undecryptable > 0 {
        if strict && undecryptable_stops_startup(&sum) {
            bail!(
                "復号できない口座番号が {} 件あるため、起動を止めます（暗号化済み {} 件中 / 全 {} 件中。鍵の不一致か表の破損の疑い）。\
                 鍵(DATA_KEYS_FILE)と鍵IDを確認してください(値は出しません)",
                sum.undecryptable, sum.encrypted, sum.total
            );
        }
        if strict {
            // 少数の破損: 起動は続けるが、人が動けるように 1 起動 1 回だけ High で知らせる
            tracing::warn!("[データ暗号化] 復号できない口座番号が {} 件あります(少数のため起動は続けます。該当の行だけ読めません)", sum.undecryptable);
            let incident = chat_notifier::Incident::new(
                integration_core::Level::High,
                format!("口座番号の一部({} 件)を復号できません", sum.undecryptable),
                "該当の口座番号だけ、画面・帳票で読めません(他の行と起動には影響しません)",
                "鍵(DATA_KEYS_FILE)と鍵IDを確認し、該当の行を直してください。値は通知に含めていません",
            );
            chat_notifier::post(&incident).await;
        } else {
            tracing::warn!("[データ暗号化] 復号できない口座番号が {} 件あります(開発環境のため起動は続けます)", sum.undecryptable);
        }
    }
    if sum.plaintext_nonempty > 0 {
        if dc.required() {
            bail!(
                "平文の口座番号が {} 件あり、DATA_ENCRYPTION_REQUIRED=true のため起動を止めます。先に変換(encrypt-bank-accounts --execute)してください",
                sum.plaintext_nonempty
            );
        }
        tracing::warn!("[データ暗号化] 平文の口座番号が {} 件あります(移行期間中)。変換してから REQUIRED=true にしてください", sum.plaintext_nonempty);
    }
    Ok(sum)
}

/// 件数だけを数える(読み取りのみ)。enc1: の行は、実際に復号して試す(値は出さない)
pub async fn scan(pool: &PgPool, dc: &DataCrypto) -> anyhow::Result<Vec<Count>> {
    let current = dc.cipher().map(|c| c.current_key_id().to_string());
    let mut conn = pool.acquire().await?;
    let mut out = Vec::new();
    for t in TARGETS {
        let rows = bank_account_repo::list_all(&mut conn, t).await?;
        let mut c = Count {
            target: t,
            total: rows.len(),
            encrypted: 0,
            plaintext: 0,
            plaintext_empty: 0,
            other_key: 0,
            undecryptable: 0,
        };
        for r in &rows {
            if DataCrypto::is_encrypted(&r.value) {
                c.encrypted += 1;
                match dc.decrypt(t.label(), &r.value) {
                    Ok(_) => {
                        if let Some(cur) = current.as_deref() {
                            if DataCrypto::key_id_of(&r.value) != Some(cur) {
                                c.other_key += 1;
                            }
                        }
                    }
                    Err(_) => c.undecryptable += 1,
                }
            } else {
                c.plaintext += 1;
                if r.value.is_empty() {
                    c.plaintext_empty += 1;
                }
            }
        }
        out.push(c);
    }
    Ok(out)
}

/// 変換の本体の前に、ゲート(`execute_gate`)を通す。ゲートを通らなければ、DB には触らず Err を返す。
pub async fn convert(pool: &PgPool, dc: &DataCrypto) -> anyhow::Result<Vec<(Target, usize)>> {
    execute_gate(dc).map_err(|e| anyhow!(e))?;
    convert_body(pool, dc).await
}

/// 平文の行(と、今の鍵以外で暗号化された行)を、今の鍵で暗号化して書き戻す。返り値は表ごとに書き換えた件数。
/// 途中で 1 件でも失敗・検証不一致・復号不能があれば、トランザクションを取り消して Err を返す(DB は変わらない)。
/// ゲートを通さない。通常は `convert` を使う(試験から直接呼ぶためだけに公開している)。
#[doc(hidden)]
pub async fn convert_body(pool: &PgPool, dc: &DataCrypto) -> anyhow::Result<Vec<(Target, usize)>> {
    if dc.cipher().is_none() {
        bail!("暗号化の鍵が読めていないため、実行できません(DATA_KEYS_FILE を確認してください)");
    }

    let current = dc.cipher().map(|c| c.current_key_id().to_string()).unwrap_or_default();
    let mut tx = pool.begin().await?;
    let mut converted: Vec<(Target, usize)> = Vec::new();
    let mut undecryptable: Vec<(Target, usize)> = Vec::new();
    // 検証用: 書き換えた行の(表, 主キー, 元の値)。値はメモリ上だけに置き、表示しない
    let mut written: Vec<(Target, String, String)> = Vec::new();

    for t in TARGETS {
        bank_account_repo::lock_table(&mut tx, t).await?;
        let rows = bank_account_repo::list_all(&mut tx, t).await?;
        let mut n = 0usize;
        let mut bad = 0usize;
        for row in rows {
            // 暗号化済み(enc1:)の行は、まず復号して確かめる。開けない行は件数に数えて止める(値は触らない)
            let plain = if DataCrypto::is_encrypted(&row.value) {
                match dc.decrypt(t.label(), &row.value) {
                    Ok(plain) => {
                        if DataCrypto::key_id_of(&row.value) == Some(current.as_str()) {
                            continue; // 今の鍵で暗号化済み(冪等)
                        }
                        plain // 旧鍵の行は、いったん復号した値を今の鍵で暗号化し直す
                    }
                    Err(_) => {
                        bad += 1;
                        continue;
                    }
                }
            } else {
                row.value.clone()
            };
            let enc = dc
                .encrypt(t.label(), &plain)
                .map_err(|e| anyhow!("{}: 暗号化できませんでした: {e}", t.label()))?;
            bank_account_repo::update_value(&mut tx, t, &row.key, &enc).await?;
            written.push((t, row.key, plain));
            n += 1;
        }
        if bad > 0 {
            undecryptable.push((t, bad));
        }
        converted.push((t, n));
    }

    // 復号できない行が 1 件でもあれば、何も書かずに止める(コミットしないので DB は変わらない)
    if !undecryptable.is_empty() {
        let detail: Vec<String> = undecryptable.iter().map(|(t, n)| format!("{} {} 件", t.label(), n)).collect();
        bail!(
            "復号できない行があるため止めました（DB は変更していません）: {}。\
             鍵が足りない・鍵IDが違う・値が壊れている可能性があります。値は出しません",
            detail.join(" / ")
        );
    }

    // 書き戻した後に読み直し、復号すると元の値と一致するかを確かめる(1 件でも違えば全体を取り消す)
    for t in TARGETS {
        let stored: HashMap<String, String> = bank_account_repo::list_all(&mut tx, t)
            .await?
            .into_iter()
            .map(|r| (r.key, r.value))
            .collect();
        for (_, key, plain) in written.iter().filter(|(wt, _, _)| *wt == t) {
            let back = stored.get(key).ok_or_else(|| anyhow!("{}: 書き戻した行が見つかりません。取り消しました", t.label()))?;
            if !DataCrypto::is_encrypted(back) {
                bail!("{}: 書き戻した値が暗号化されていません。すべての変更を取り消しました", t.label());
            }
            let ok = dc.read_value(t.label(), back).map(|v| v == *plain).unwrap_or(false);
            if !ok {
                bail!("{}: 復号の検証が一致しません。すべての変更を取り消しました(値は出しません)", t.label());
            }
        }
    }

    tx.commit().await?;
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    // このビルドは、暗号文を読めるので、変換の実行を許す版であること(変えるなら、この行を消して理由を設計書に書く)
    const _: () = assert!(APP_READS_ENCRYPTED, "S2-G2-5 以降は変換の実行を許す版であること");

    fn sum(total: usize, encrypted: usize, undecryptable: usize) -> StartupCheck {
        StartupCheck { total, encrypted, undecryptable, ..Default::default() }
    }

    #[test]
    fn stops_when_every_encrypted_row_fails_key_mismatch() {
        // 暗号化済みの行が全部開けない(鍵の不一致の疑い)。全行数が多くても止める
        assert!(undecryptable_stops_startup(&sum(1000, 1, 1)), "1 件だけでも全部開けないなら止める");
        assert!(undecryptable_stops_startup(&sum(10, 4, 4)));
    }

    #[test]
    fn stops_when_the_majority_of_all_rows_fail() {
        // 暗号化済み 3 件のうち 2 件が開けない(過半数)。全行は 3 件
        assert!(undecryptable_stops_startup(&sum(3, 3, 2)), "全行 3 件中 2 件(過半数)は止める");
        assert!(undecryptable_stops_startup(&sum(4, 4, 3)));
    }

    #[test]
    fn minority_corruption_does_not_stop() {
        // 全行 4 件、暗号化済み 4 件、開けないのは 1 件(少数)。起動は続ける
        assert!(!undecryptable_stops_startup(&sum(4, 4, 1)), "1 件の破損は起動を止めない");
        // 平文が多い表で、暗号文 5 件のうち 1 件だけ壊れている(全行 100 件中 1 件)は少数の破損として続ける
        assert!(!undecryptable_stops_startup(&sum(100, 5, 1)), "全 100 件中の 1 件は少数");
        assert!(!undecryptable_stops_startup(&sum(5, 4, 0)), "復号できない行が無ければ止めない");
    }

    #[test]
    fn gate_needs_the_app_the_environment_and_the_key_all_together() {
        for app in [false, true] {
            for env in [false, true] {
                for key in [false, true] {
                    let ok = gate_rule(env, app, key).is_ok();
                    assert_eq!(ok, app && env && key, "app={app} env={env} key={key}");
                }
            }
        }
    }

    #[test]
    fn gate_refusal_reasons_are_japanese_and_do_not_carry_values() {
        let e = gate_rule(true, false, true).unwrap_err();
        assert!(e.contains("拒否"), "アプリが読めない: {e}");
        let e = gate_rule(false, true, true).unwrap_err();
        assert!(e.contains("DATA_ENCRYPTION_ENABLED"), "環境で有効化していない: {e}");
        let e = gate_rule(true, true, false).unwrap_err();
        assert!(e.contains("鍵"), "鍵が無い: {e}");
    }

    #[tokio::test]
    async fn convert_refuses_before_touching_the_database() {
        // 接続しない(遅延接続)プール。ゲートで止まれば、接続を試みない = DB に触らない。環境変数は読まない
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://sophia:none@127.0.0.1:1/none")
            .expect("遅延接続のプールを作れること");
        // ENABLED=false の設定(鍵なし)。ゲートで拒否されること
        let dc = DataCrypto::build("local", None, false, false).expect("開発の設定");
        let err = convert(&pool, &dc).await.expect_err("ゲートで拒否されること");
        let msg = format!("{err:#}");
        assert!(msg.contains("DATA_ENCRYPTION_ENABLED"), "ゲートの理由を出す: {msg}");
    }
}
