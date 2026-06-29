//! Persisted configuration for the Ark connection.
//!
//! The API key is sensitive; we keep it in the same store but never log it and
//! never return it to the frontend (the UI only sends it, and asks whether one
//! is set). The store file lives in the app config dir via tauri-plugin-store.

use crate::ark::ArkConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArkSettings {
    pub base_url: String,
    /// Inference endpoint id (`ep-xxxx`) or model name.
    pub model: String,
    #[serde(default = "default_temp")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// API protocol: "anthropic" (`/api/coding` + `/v1/messages`) or "openai"
    /// (`/api/coding/v3` + `/chat/completions`). The console-managed
    /// `ark-code-latest` alias only resolves on the Anthropic endpoint.
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Stored secret. Skipped when serialising to the frontend.
    #[serde(default)]
    pub api_key: String,
}

fn default_protocol() -> String {
    "anthropic".into()
}

fn default_temp() -> f32 {
    0.7
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_concurrency() -> usize {
    6
}
fn default_timeout() -> u64 {
    120
}

impl Default for ArkSettings {
    fn default() -> Self {
        Self {
            // Ark Coding Plan, Anthropic protocol — this is what Claude Code
            // uses and where the `ark-code-latest` alias resolves.
            base_url: "https://ark.cn-beijing.volces.com/api/coding".into(),
            model: "ark-code-latest".into(),
            temperature: default_temp(),
            max_tokens: default_max_tokens(),
            max_concurrency: default_concurrency(),
            timeout_secs: default_timeout(),
            protocol: default_protocol(),
            // Secret is never baked into source; entered in Settings and stored
            // locally. May also be seeded at runtime via the ARK_API_KEY env var.
            api_key: std::env::var("ARK_API_KEY").unwrap_or_default(),
        }
    }
}

impl ArkSettings {
    /// Build a runtime Ark client config from these settings.
    pub fn to_ark_config(&self) -> ArkConfig {
        ArkConfig {
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            timeout_secs: self.timeout_secs,
            max_retries: 2,
        }
    }

    pub fn is_anthropic(&self) -> bool {
        self.protocol.eq_ignore_ascii_case("anthropic")
    }

    /// A redacted view safe to send to the frontend (no secret).
    pub fn redacted(&self) -> serde_json::Value {
        serde_json::json!({
            "base_url": self.base_url,
            "model": self.model,
            "protocol": self.protocol,
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
            "max_concurrency": self.max_concurrency,
            "timeout_secs": self.timeout_secs,
            "has_api_key": !self.api_key.trim().is_empty(),
        })
    }
}
