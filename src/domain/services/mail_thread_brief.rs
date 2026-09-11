/// domain/services/mail_thread_brief.rs — 取引先メールスレッド要約の構造化生成
///
/// Ollama が使えない場合に備えた、メールスレッド情報から構造化要約を生成する純関数群。
/// UI や Ollama プロンプトから共用される。

use sqlx::types::Decimal;

/// メール添付ファイルの種別分類
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    /// 見込み報告（ファイル名に「見込み」を含む）
    #[serde(rename = "forecast")]
    Forecast,
    /// 月末・最終報告（ファイル名に「月末」または「最終」を含む）
    #[serde(rename = "final")]
    Final,
    /// 勤務表・稼働報告（見込み/最終が判別できない）
    #[serde(rename = "timesheet")]
    Timesheet,
    /// その他の添付ファイル
    #[serde(rename = "other")]
    Other,
}

/// ファイル名と件名からAttachmentKindを判定
pub fn attachment_kind(filename: &str, subject: &str) -> AttachmentKind {
    let text = format!("{} {}", filename, subject).to_lowercase();

    if text.contains("見込み") {
        AttachmentKind::Forecast
    } else if text.contains("月末") || text.contains("最終") {
        AttachmentKind::Final
    } else if text.contains("勤務表") || text.contains("稼働") {
        AttachmentKind::Timesheet
    } else {
        AttachmentKind::Other
    }
}

/// Decimal（小数時間）を「XXX時間YY分」の文字列に整形
/// 例: Decimal::from(198.5) → "198時間30分"
/// 例: Decimal::from(177.5) → "177時間30分"
pub fn format_hours(hours: Decimal) -> String {
    let whole = hours.trunc().to_string().parse::<i32>().unwrap_or(0);
    let frac = (hours - Decimal::from(whole)) * Decimal::from(60);
    let minutes = frac.round().to_string().parse::<i32>().unwrap_or(0);

    if minutes == 60 {
        format!("{}時間00分", whole + 1)
    } else {
        format!("{}時間{}分", whole, minutes)
    }
}

/// メール件名から Re:/RE:/Fw:/Fwd: の繰り返しを除去（大小文字無視）
pub fn normalize_subject(subject: &str) -> String {
    let mut s = subject.trim().to_string();

    loop {
        let lower = s.to_lowercase();
        let trimmed = if lower.starts_with("re:") {
            s[3..].trim()
        } else if lower.starts_with("fw:") {
            s[3..].trim()
        } else if lower.starts_with("fwd:") {
            s[4..].trim()
        } else {
            break;
        };

        s = trimmed.to_string();
    }

    s
}

/// 相手別スレッド情報から構造化要約を生成
///
/// 入力:
/// - from_name: 相手の名前（例："長塚彩香"）
/// - company_name: 会社名（例："クロスシステムサービス株式会社"）
/// - email_count: 該当メール総数
/// - attachment_items: [(filename, kind, hours_label, timesheet_status), ...]
pub fn build_structured_summary(
    from_name: &str,
    company_name: &str,
    email_count: usize,
    attachments: &[(String, AttachmentKind, Option<String>, Option<String>)],
) -> String {
    let mut lines = vec![];

    if company_name.is_empty() {
        lines.push(format!("{}さん（会社名不明）とのやり取り。", from_name));
    } else {
        lines.push(format!("{}さん（{}）とのやり取り。", from_name, company_name));
    }

    // 添付がある場合はそれを優先表示
    if !attachments.is_empty() {
        // 最終・見込みを分離
        let finals: Vec<_> = attachments.iter().filter(|(_, k, _, _)| *k == AttachmentKind::Final).collect();
        let forecasts: Vec<_> = attachments.iter().filter(|(_, k, _, _)| *k == AttachmentKind::Forecast).collect();

        if !finals.is_empty() {
            for (filename, _, hours, status) in &finals {
                let hours_str = hours.as_ref().map(|h| format!("（{}）", h)).unwrap_or_default();
                let status_str = status.as_ref().map(|s| format!(" — {}", s)).unwrap_or_else(|| " — 登録済み".to_string());
                lines.push(format!("・最終: {}{}{}", filename, hours_str, status_str));
            }
        }

        if !forecasts.is_empty() {
            for (filename, _, hours, status) in &forecasts {
                let hours_str = hours.as_ref().map(|h| format!("（{}）", h)).unwrap_or_default();
                let status_str = status.as_ref().map(|_| "反映は未選択").unwrap_or_else(|| "反映は未選択");
                lines.push(format!("・見込み: {}{}  — 添付あり（{}）", filename, hours_str, status_str));
            }
        }

        // その他の添付
        let others: Vec<_> = attachments.iter().filter(|(_, k, _, _)| *k == AttachmentKind::Other).collect();
        if !others.is_empty() {
            for (filename, _, _, _) in &others {
                lines.push(format!("・その他: {}", filename));
            }
        }
    } else if email_count > 0 {
        lines.push(format!("メールが{}通あります。", email_count));
        lines.push("本文の自動要約は準備中です。".to_string());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attachment_kind_forecast() {
        assert_eq!(
            attachment_kind("勤務表_2026年08月_見込み報告.pdf", "勤務表の件"),
            AttachmentKind::Forecast
        );
    }

    #[test]
    fn test_attachment_kind_final() {
        assert_eq!(
            attachment_kind("勤務表_2026年08月_月末報告.pdf", "勤務表の件"),
            AttachmentKind::Final
        );
        assert_eq!(
            attachment_kind("勤務表_2026年08月_最終報告.pdf", "勤務表の件"),
            AttachmentKind::Final
        );
    }

    #[test]
    fn test_attachment_kind_timesheet() {
        assert_eq!(
            attachment_kind("report_20260828.pdf", "勤務表"),
            AttachmentKind::Timesheet
        );
    }

    #[test]
    fn test_format_hours_half_hour() {
        // 198.5 → 198時間30分
        let hours = Decimal::new(1985, 1); // 198.5
        assert_eq!(format_hours(hours), "198時間30分");
    }

    #[test]
    fn test_format_hours_zero_minutes() {
        assert_eq!(format_hours(Decimal::from(200)), "200時間0分");
    }

    #[test]
    fn test_format_hours_rounding() {
        // 177.5 → 177時間30分
        let hours = Decimal::new(1775, 1); // 177.5
        assert_eq!(format_hours(hours), "177時間30分");
    }

    #[test]
    fn test_normalize_subject() {
        assert_eq!(normalize_subject("Re: 勤務表の件"), "勤務表の件");
        assert_eq!(normalize_subject("RE: FW: 案件について"), "案件について");
        assert_eq!(normalize_subject("Fwd: 契約について"), "契約について");
    }

    #[test]
    fn test_build_structured_summary_with_attachments() {
        let attachments = vec![
            (
                "勤務表_2026年08月_月末報告.pdf".to_string(),
                AttachmentKind::Final,
                Some("177時間30分".to_string()),
                Some("登録済み".to_string()),
            ),
            (
                "勤務表_2026年08月_見込み報告.pdf".to_string(),
                AttachmentKind::Forecast,
                None,
                None,
            ),
        ];

        let summary = build_structured_summary("長塚彩香", "クロスシステムサービス株式会社", 4, &attachments);
        assert!(summary.contains("長塚彩香さん"), "Summary should contain name: {}", summary);
        assert!(summary.contains("クロスシステムサービス株式会社"), "Summary should contain company");
        assert!(summary.contains("最終"), "Summary should contain '最終'");
        assert!(summary.contains("見込み"), "Summary should contain '見込み'");
    }

    #[test]
    fn test_build_structured_summary_no_attachments() {
        let attachments: Vec<_> = vec![];
        let summary = build_structured_summary("宇田川哲宏", "クロスシステムサービス株式会社", 3, &attachments);
        assert!(summary.contains("宇田川哲宏さん"));
        assert!(summary.contains("メールが3通あります"));
        assert!(summary.contains("自動要約は準備中"));
    }
}
