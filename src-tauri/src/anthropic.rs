//! Ark (火山方舟) Coding Plan — Anthropic-protocol client.
//!
//! Ark's Coding Plan exposes the SAME model alias `ark-code-latest` via two
//! protocols. The console-managed alias resolves on the Anthropic endpoint
//! (`/api/coding` + `/v1/messages`) but NOT on the OpenAI one — using the
//! OpenAI path with that alias yields `InvalidEndpointOrModel.NotFound` (404).
//! Claude Code itself uses this Anthropic endpoint, which is why the same key
//! and model work there. This client mirrors that path.
//!
//!   - base_url default `https://ark.cn-beijing.volces.com/api/coding`
//!   - path `/v1/messages`
//!   - header `Authorization: Bearer <ARK_API_KEY>` (+ `anthropic-version`)
//!   - `model` e.g. `ark-code-latest`

use crate::ark::ArkConfig;
use crate::llm::{ChatRequest, ChunkSink, LlmClient, LlmError};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;

pub struct AnthropicClient {
    cfg: ArkConfig,
    http: reqwest::Client,
}

impl AnthropicClient {
    pub fn new(cfg: ArkConfig) -> Result<Self, LlmError> {
        if cfg.api_key.trim().is_empty() {
            return Err(LlmError::Config("api_key 未配置".into()));
        }
        // Use an *idle* (read) timeout, not a total-request timeout: large
        // synthesis/validation nodes can stream for minutes. We only want to
        // abort if the server goes silent, not because the whole job is long.
        let http = reqwest::Client::builder()
            .read_timeout(Duration::from_secs(cfg.timeout_secs))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        Ok(Self { cfg, http })
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.cfg.base_url.trim_end_matches('/'))
    }

    fn model_for(&self, req: &ChatRequest) -> String {
        if req.model.trim().is_empty() {
            self.cfg.model.clone()
        } else {
            req.model.clone()
        }
    }

    /// Build the Anthropic Messages body: split a leading system message into
    /// the top-level `system` field; the rest become user/assistant turns.
    fn build_body(&self, req: &ChatRequest, stream: bool) -> serde_json::Value {
        let mut system = String::new();
        let mut messages = Vec::new();
        for m in &req.messages {
            if m.role == "system" {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            } else {
                messages.push(serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                }));
            }
        }
        let mut body = serde_json::json!({
            "model": self.model_for(req),
            "messages": messages,
            "max_tokens": req.max_tokens.unwrap_or(4096),
            "stream": stream,
        });
        if !system.is_empty() {
            body["system"] = serde_json::Value::String(system);
        }
        if let Some(t) = req.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        body
    }
}

#[derive(Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}
#[derive(Deserialize)]
struct ContentBlock {
    #[serde(default)]
    text: String,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, req: &ChatRequest) -> Result<String, LlmError> {
        let body = self.build_body(req, false);
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Http { status, body });
        }
        let parsed: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| LlmError::Decode(e.to_string()))?;
        Ok(parsed.content.into_iter().map(|c| c.text).collect())
    }

    async fn stream(
        &self,
        req: &ChatRequest,
        on_chunk: &mut ChunkSink<'_>,
    ) -> Result<String, LlmError> {
        let body = self.build_body(req, true);
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.cfg.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Http { status, body });
        }

        let mut full = String::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(b) => b,
                Err(e) => {
                    // A read error AFTER we've already received content is almost
                    // always a benign tail (idle read-timeout once the model
                    // finished, or connection close after the final SSE frame).
                    // Keep what we streamed rather than failing the node. Only a
                    // failure with zero content is a real error.
                    if full.is_empty() {
                        return Err(LlmError::Network(e.to_string()));
                    }
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                if let Some(delta) = parse_anthropic_sse(&line) {
                    match delta {
                        Delta::Text(t) => {
                            on_chunk(t.clone());
                            full.push_str(&t);
                        }
                        // Thinking is shown live but not part of the output.
                        Delta::Thinking(t) => on_chunk(t),
                    }
                }
            }
        }
        Ok(full)
    }
}

/// A parsed streaming delta: either visible answer text, or reasoning/thinking
/// content. Thinking is shown in the UI (so long synthesis nodes visibly
/// progress) but excluded from the node's returned output.
pub enum Delta {
    Text(String),
    Thinking(String),
}

/// Parse one Anthropic SSE `data:` line into a `Delta`, if it carries content.
/// Handles `text_delta` and `thinking_delta` content_block_delta events.
pub fn parse_anthropic_sse(line: &str) -> Option<Delta> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    let delta = v.get("delta")?;
    // Reasoning models emit thinking before the answer.
    if let Some(t) = delta.get("thinking").and_then(|x| x.as_str()) {
        if !t.is_empty() {
            return Some(Delta::Thinking(t.to_string()));
        }
    }
    // {"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}
    let text = delta.get("text")?.as_str()?;
    if text.is_empty() {
        None
    } else {
        Some(Delta::Text(text.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta() {
        let line = r#"data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"你好"}}"#;
        match parse_anthropic_sse(line) {
            Some(Delta::Text(t)) => assert_eq!(t, "你好"),
            _ => panic!("expected text delta"),
        }
    }

    #[test]
    fn parses_thinking_delta() {
        let line = r#"data: {"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"嗯..."}}"#;
        match parse_anthropic_sse(line) {
            Some(Delta::Thinking(t)) => assert_eq!(t, "嗯..."),
            _ => panic!("expected thinking delta"),
        }
    }

    #[test]
    fn ignores_non_text_events() {
        assert!(parse_anthropic_sse("data: [DONE]").is_none());
        assert!(parse_anthropic_sse(": ping").is_none());
        let start = r#"data: {"type":"message_start","message":{}}"#;
        assert!(parse_anthropic_sse(start).is_none());
    }
}
