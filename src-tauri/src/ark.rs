//! Ark (火山方舟) OpenAI-compatible client.
//!
//! Ark exposes an OpenAI-compatible Chat Completions API. All connection
//! parameters are configurable so the coding-plan endpoint drops straight in:
//!   - base_url default `https://ark.cn-beijing.volces.com/api/v3`
//!   - path `/chat/completions`
//!   - header `Authorization: Bearer <ARK_API_KEY>`
//!   - `model` = inference endpoint id (`ep-xxxx`) or model name

use crate::llm::{ChatRequest, ChunkSink, LlmClient, LlmError};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ArkConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for ArkConfig {
    fn default() -> Self {
        Self {
            base_url: "https://ark.cn-beijing.volces.com/api/v3".into(),
            api_key: String::new(),
            model: String::new(),
            timeout_secs: 120,
            max_retries: 2,
        }
    }
}

pub struct ArkClient {
    cfg: ArkConfig,
    http: reqwest::Client,
}

impl ArkClient {
    pub fn new(cfg: ArkConfig) -> Result<Self, LlmError> {
        if cfg.api_key.trim().is_empty() {
            return Err(LlmError::Config("api_key 未配置".into()));
        }
        let http = reqwest::Client::builder()
            .read_timeout(Duration::from_secs(cfg.timeout_secs))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        Ok(Self { cfg, http })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.cfg.base_url.trim_end_matches('/'))
    }

    fn models_endpoint(&self) -> String {
        format!("{}/models", self.cfg.base_url.trim_end_matches('/'))
    }

    /// Query the OpenAI-compatible `GET /models` endpoint for the available
    /// models / inference endpoints on this account. Used to give the user a
    /// concrete list when their configured model is wrong (404).
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let resp = self
            .http
            .get(self.models_endpoint())
            .bearer_auth(&self.cfg.api_key)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Http { status, body });
        }
        let parsed: ModelList = resp
            .json()
            .await
            .map_err(|e| LlmError::Decode(e.to_string()))?;
        Ok(parsed.data.into_iter().map(|m| m.id).collect())
    }

    /// Apply the configured model unless the request already pins one.
    fn resolve(&self, req: &ChatRequest) -> ChatRequest {
        let mut r = req.clone();
        if r.model.trim().is_empty() {
            r.model = self.cfg.model.clone();
        }
        r
    }
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelEntry>,
}
#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: Option<MessageBody>,
    delta: Option<DeltaBody>,
}
#[derive(Deserialize)]
struct MessageBody {
    #[serde(default)]
    content: String,
}
#[derive(Deserialize)]
struct DeltaBody {
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl LlmClient for ArkClient {
    async fn complete(&self, req: &ChatRequest) -> Result<String, LlmError> {
        let mut body = self.resolve(req);
        body.stream = false;

        let mut attempt = 0;
        loop {
            let resp = self
                .http
                .post(self.endpoint())
                .bearer_auth(&self.cfg.api_key)
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    let parsed: CompletionResponse = r
                        .json()
                        .await
                        .map_err(|e| LlmError::Decode(e.to_string()))?;
                    let text = parsed
                        .choices
                        .into_iter()
                        .find_map(|c| c.message.map(|m| m.content))
                        .unwrap_or_default();
                    return Ok(text);
                }
                Ok(r) => {
                    let status = r.status().as_u16();
                    let bodytext = r.text().await.unwrap_or_default();
                    // Retry only on transient server-side / rate-limit errors.
                    if (status == 429 || status >= 500) && attempt < self.cfg.max_retries {
                        attempt += 1;
                        backoff(attempt).await;
                        continue;
                    }
                    return Err(LlmError::Http { status, body: bodytext });
                }
                Err(e) => {
                    if attempt < self.cfg.max_retries {
                        attempt += 1;
                        backoff(attempt).await;
                        continue;
                    }
                    return Err(LlmError::Network(e.to_string()));
                }
            }
        }
    }

    async fn stream(
        &self,
        req: &ChatRequest,
        on_chunk: &mut ChunkSink<'_>,
    ) -> Result<String, LlmError> {
        let mut body = self.resolve(req);
        body.stream = true;

        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let bodytext = resp.text().await.unwrap_or_default();
            return Err(LlmError::Http { status, body: bodytext });
        }

        let mut full = String::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    // Tail read error after content already streamed is benign
                    // (idle timeout / connection close after the last frame).
                    // Keep what we have; only fail if we got nothing.
                    if full.is_empty() {
                        return Err(LlmError::Network(e.to_string()));
                    }
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // SSE frames are separated by newlines; each data line is JSON.
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if let Some(delta) = parse_sse_line(&line) {
                    on_chunk(delta.clone());
                    full.push_str(&delta);
                }
            }
        }
        Ok(full)
    }
}

/// Parse one SSE `data:` line, returning the content delta if present.
/// Exposed at crate level so unit tests can verify stream parsing without a
/// live connection. Returns `None` for `[DONE]`, comments, and empty deltas.
pub fn parse_sse_line(line: &str) -> Option<String> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let parsed: CompletionResponse = serde_json::from_str(payload).ok()?;
    let delta = parsed
        .choices
        .into_iter()
        .find_map(|c| c.delta.and_then(|d| d.content))?;
    if delta.is_empty() {
        None
    } else {
        Some(delta)
    }
}

async fn backoff(attempt: u32) {
    // Exponential backoff: 200ms, 400ms, 800ms, ...
    let ms = 200u64 * 2u64.pow(attempt.saturating_sub(1));
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_streamed_delta() {
        let line = r#"data: {"choices":[{"delta":{"content":"你好"}}]}"#;
        assert_eq!(parse_sse_line(line), Some("你好".to_string()));
    }

    #[test]
    fn ignores_done_and_empty() {
        assert_eq!(parse_sse_line("data: [DONE]"), None);
        assert_eq!(parse_sse_line(""), None);
        assert_eq!(parse_sse_line(": keep-alive"), None);
        let empty = r#"data: {"choices":[{"delta":{}}]}"#;
        assert_eq!(parse_sse_line(empty), None);
    }

    #[test]
    fn requires_api_key() {
        let r = ArkClient::new(ArkConfig::default());
        assert!(r.is_err());
    }
}
