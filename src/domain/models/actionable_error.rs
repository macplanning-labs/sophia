use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBlocker {
    pub subject: String,
    pub context: String,
    pub reason: String,
    pub suggestion: String,
    pub link_path: String,
    pub link_label: String,
    pub code: String,
}

pub fn format_action_blockers_message(blockers: &[ActionBlocker], fallback: &str) -> String {
    if blockers.is_empty() {
        return fallback.to_string();
    }

    blockers
        .iter()
        .map(|b| {
            let mut line = format!("【{}】{}", b.subject, b.reason);
            if !b.context.is_empty() {
                line = format!("【{}】{}", b.context, line);
            }
            line.push_str(&format!("\n→ {}（{}）", b.suggestion, b.link_label));
            line
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_empty() {
        let result = format_action_blockers_message(&[], "fallback");
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_format_single() {
        let blocker = ActionBlocker {
            subject: "test".to_string(),
            context: "ctx".to_string(),
            reason: "reason".to_string(),
            suggestion: "suggestion".to_string(),
            link_path: "/path".to_string(),
            link_label: "label".to_string(),
            code: "test_code".to_string(),
        };
        let result = format_action_blockers_message(&[blocker], "fallback");
        assert!(result.contains("【ctx】【test】"));
        assert!(result.contains("→ suggestion（label）"));
    }

    #[test]
    fn test_format_multiple() {
        let blockers = vec![
            ActionBlocker {
                subject: "subj1".to_string(),
                context: "ctx1".to_string(),
                reason: "reason1".to_string(),
                suggestion: "sugg1".to_string(),
                link_path: "/p1".to_string(),
                link_label: "l1".to_string(),
                code: "code1".to_string(),
            },
            ActionBlocker {
                subject: "subj2".to_string(),
                context: "ctx2".to_string(),
                reason: "reason2".to_string(),
                suggestion: "sugg2".to_string(),
                link_path: "/p2".to_string(),
                link_label: "l2".to_string(),
                code: "code2".to_string(),
            },
        ];
        let result = format_action_blockers_message(&blockers, "fallback");
        assert!(result.contains("ctx1"));
        assert!(result.contains("ctx2"));
        assert!(result.contains("\n\n"));
    }
}
