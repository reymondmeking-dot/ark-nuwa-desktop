//! LLM abstraction layer.
//!
//! The whole kernel talks to an `LlmClient` trait, never to a concrete HTTP
//! client. This is the seam that lets `cargo test` exercise the DAG engine and
//! the full distillation loop with a deterministic `MockClient` and zero
//! network access.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// One message in an OpenAI-compatible chat request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

/// A chat completion request, mapped 1:1 onto the OpenAI-compatible body Ark
/// expects. `model` is the Ark inference endpoint id (e.g. `ep-xxxx`) or a
/// model name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            stream: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("http error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("missing configuration: {0}")]
    Config(String),
}

/// Callback invoked for each streamed chunk of content. Takes an owned
/// `String` (rather than `&str`) so the future returned by `stream` carries no
/// higher-ranked lifetime — this keeps it usable from async Tauri commands.
pub type ChunkSink<'a> = dyn FnMut(String) + Send + 'a;

/// The single abstraction every part of the kernel depends on.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Non-streaming completion. Returns the full assistant text. The DAG
    /// engine uses this for deterministic node execution.
    async fn complete(&self, req: &ChatRequest) -> Result<String, LlmError>;

    /// Streaming completion. Invokes `on_chunk` for each delta and returns the
    /// fully concatenated text. The chat session uses this for live UX.
    async fn stream(
        &self,
        req: &ChatRequest,
        on_chunk: &mut ChunkSink<'_>,
    ) -> Result<String, LlmError>;
}
