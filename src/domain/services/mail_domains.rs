//! 「取引先のメール」として扱わないドメイン(自社のドメインなど)の設定の読み取り。
//! 設定は自社情報(`s_company_info.excluded_mail_domains`)に、1行に1ドメインで持つ(カンマ・空白区切りも可)。

/// 設定の文字列を、小文字・重複なしのドメイン一覧にする。`@` と空の要素は取り除く。
pub fn parse_excluded_domains(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in raw.split(|c: char| c == '\n' || c == '\r' || c == ',' || c == ';' || c.is_whitespace()) {
        let d = token.trim().trim_start_matches('@').to_lowercase();
        if d.is_empty() || !d.contains('.') {
            continue;
        }
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_lines_commas_and_spaces_and_lowercases() {
        assert_eq!(
            parse_excluded_domains("MacPlanning.com\n@example.co.jp, foo.example.com ; bar.test"),
            vec!["macplanning.com", "example.co.jp", "foo.example.com", "bar.test"]
        );
    }

    #[test]
    fn drops_empty_duplicates_and_tokens_without_a_dot() {
        assert_eq!(parse_excluded_domains(""), Vec::<String>::new());
        assert_eq!(parse_excluded_domains("  \n\n"), Vec::<String>::new());
        assert_eq!(parse_excluded_domains("a.com\nA.COM\nlocalhost"), vec!["a.com"]);
    }
}
