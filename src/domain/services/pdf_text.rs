//! PDF テキスト抽出（panic 検出・防御付き）
//!
//! `pdf_extract::extract_text_from_mem` は、特定のエンコーディング形式（例: 日本語の `UniJIS-UCS2-H`）の
//! PDF で panic することがある（旧版・新版とも確認済み）。ここでは panic を `Err` に変換し、
//! 呼び出し側で通常のエラーとして扱えるようにする。`catch_unwind` は panic=unwind（本プロジェクトの既定）で有効。

use anyhow::{anyhow, Result};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// panic を検出して Result に変換する汎用ヘルパ
fn run_catching<T: 'static>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => Ok(result),
        Err(_) => {
            let msg = "PDFの解析中に内部エラー（非対応の形式の可能性）";
            tracing::error!("{}", msg);
            Err(anyhow!(msg))
        }
    }
}

/// PDF バイト列からテキストを抽出
///
/// `pdf_extract::extract_text_from_mem` を呼び出し、panic と通常エラーを `anyhow::Result` に統一。
///
/// # 引数
/// * `bytes` - PDF のバイト列
///
/// # 戻り値
/// 正常終了時は抽出したテキスト、エラー時は `Err`
pub fn extract_text(bytes: &[u8]) -> Result<String> {
    let bytes_owned = bytes.to_vec();
    run_catching(move || pdf_extract::extract_text_from_mem(&bytes_owned))
        .and_then(|result| {
            result.map_err(|e| anyhow!("PDF テキスト抽出エラー: {}", e))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_catching_with_panic() {
        // panic を Err に変換
        let result = run_catching(|| panic!("test panic"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("PDFの解析中に内部エラー"));
    }

    #[test]
    fn test_run_catching_with_ok() {
        // 正常系は Ok を返す
        let result = run_catching(|| 42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_extract_text_invalid_pdf() {
        // 壊れたバイト列は Err を返す（panic しない）
        let invalid_pdf = b"not a pdf";
        let result = extract_text(invalid_pdf);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_text_sample_pdf() {
        // サンプル勤務表 PDF から本文を取得
        let pdf_path = "docs/templates/勤務表_クロスシステム_サンプル.pdf";
        if let Ok(bytes) = std::fs::read(pdf_path) {
            let result = extract_text(&bytes);
            assert!(result.is_ok(), "PDF テキスト抽出失敗: {:?}", result.err());
            let text = result.unwrap();
            assert!(text.contains("勤務"), "「勤務」が含まれていない。抽出テキスト: {}", text);
        } else {
            // ファイルが無い環境ではスキップ
            eprintln!("警告: {} が見つかりません。テストをスキップします", pdf_path);
        }
    }
}
