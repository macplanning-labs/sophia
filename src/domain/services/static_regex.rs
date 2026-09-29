/// domain/services/static_regex.rs — 静的正規表現のコンパイル
///
/// 本番コード中の Regex::new(..).unwrap() / .expect() を集約する単一箇所。
/// ここだけが正規表現コンパイル失敗時に panic する。
/// パターン文字列は全て &'static str（定数）であることが前提条件。

use regex::Regex;

/// 静的（定数）正規表現パターンをコンパイルする。
///
/// このコンパイル失敗は、アプリケーション起動前にユニットテストで
/// 確実に検出されるべき設定エラー（開発時バグ）。
/// リクエスト処理中ではないため、panic で fail-fast するのが正当。
pub fn compile_static(pattern: &'static str) -> Regex {
    Regex::new(pattern)
        .expect("静的正規表現パターンはコンパイル可能（起動前テストで検証）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_static_accepts_valid_patterns() {
        let re = compile_static(r"^T\d{13}$");
        assert!(re.is_match("T1234567890123"));
        assert!(!re.is_match("T123456789012"));
    }
}
