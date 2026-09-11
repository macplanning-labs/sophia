/// infrastructure/ollama_client.rs — Ollama ローカル LLM 要約クライアント
///
/// 業務メールから自動要約を生成する。クラウド LLM は禁止（社外送信禁止）。
/// OLLAMA_HOST が設定されていない場合は呼ばない。

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Ollama 要約リクエスト
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

/// Ollama 要約レスポンス
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// メール情報から Ollama 用プロンプトを生成
fn build_prompt(from_name: &str, company_name: &str, emails: &[(String, String, String)]) -> String {
    let email_list = emails
        .iter()
        .map(|(date, subject, body_preview)| {
            format!("  - {} 「{}」\n    {}", date, subject, body_preview)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"あなたは SES 事務アシスタント。次のメールスレッドを、Gmail の「AI による概要」のような短文（3～5文）で要約してください。

相手: {}さん（{}）

最新のメール:
{}

要約を日本語で書いてください。推測で事実を足さず、メール内の実際の内容に基づいてください。"#,
        from_name, company_name, email_list
    )
}

/// Ollama ローカル LLM サービスを使用して要約を生成
///
/// OLLAMA_HOST が空文字またはセットされていない場合は None を返す。
/// タイムアウト 45 秒。失敗してもエラーを出すだけで処理を止めない。
pub async fn generate_summary(
    from_name: &str,
    company_name: &str,
    emails: &[(String, String, String)], // (date, subject, body_preview)
) -> Result<Option<String>> {
    let host = match std::env::var("OLLAMA_HOST") {
        Ok(h) if !h.is_empty() => h,
        _ => return Ok(None),
    };
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:14b".to_string());

    let prompt = build_prompt(from_name, company_name, emails);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()?;

    let req = OllamaRequest {
        model,
        prompt,
        stream: false,
    };

    let url = format!("{}/api/generate", host);
    tracing::debug!("[Ollama] POST {} (model from OLLAMA_MODEL)", url);

    let response = match client.post(&url).json(&req).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("[Ollama] リクエスト送信失敗: {}", e);
            return Ok(None);
        }
    };

    match response.json::<OllamaResponse>().await {
        Ok(resp) => {
            let summary = resp.response.trim().to_string();
            tracing::debug!("[Ollama] 要約取得成功: {} 文字", summary.len());
            Ok(Some(summary))
        }
        Err(e) => {
            tracing::error!("[Ollama] レスポンスパース失敗: {}", e);
            Ok(None)
        }
    }
}
