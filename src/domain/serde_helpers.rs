/// domain/serde_helpers.rs — フォームデシリアライズ用ヘルパー
///
/// HTMLフォームから送信される空文字列を Option::None に変換するなど、
/// フォーム構造体で共通的に使用するカスタムデシリアライザーを集約。

use chrono::NaiveDate;
use serde::Deserialize;

/// 空文字列を None に変換する日付デシリアライザー
///
/// HTMLフォームの date input は値が未入力の場合に空文字列 "" を送信する。
/// `Option<NaiveDate>` を直接デシリアライズすると "premature end of input" エラーになるため、
/// このヘルパーで空文字列を None に変換する。
///
/// ## 使用例
/// ```ignore
/// #[derive(Deserialize)]
/// pub struct MyForm {
///     #[serde(default, deserialize_with = "crate::domain::serde_helpers::deserialize_optional_date")]
///     pub some_date: Option<NaiveDate>,
/// }
/// ```
pub fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(ref v) if !v.trim().is_empty() => {
            NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d")
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
        _ => Ok(None),
    }
}
