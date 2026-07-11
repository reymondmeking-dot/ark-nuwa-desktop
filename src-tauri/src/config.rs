//! Persisted configuration for the Ark connection.
//!
//! The API key is sensitive. It is kept in the operating-system credential
//! store and is intentionally omitted from the JSON settings file and every
//! frontend response.

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
    /// Runtime secret. Deserialisation remains enabled for one-time migration
    /// from versions that wrote the key into settings.json.
    #[serde(default, skip_serializing)]
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

pub fn validate_official_ark_base_url(value: &str, expected_path: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value.trim()).map_err(|_| "Base URL 格式无效".to_string())?;
    if url.scheme() != "https" {
        return Err("Base URL 必须使用 HTTPS".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Base URL 不能包含用户名或密码".into());
    }
    if url.host_str() != Some("ark.cn-beijing.volces.com") {
        return Err("Base URL 只能使用官方 Ark 域名 ark.cn-beijing.volces.com".into());
    }
    if url.port_or_known_default() != Some(443) {
        return Err("Base URL 只能使用标准 HTTPS 端口 443".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Base URL 不能包含查询参数或片段".into());
    }
    if url.path().trim_end_matches('/') != expected_path {
        return Err(format!("Base URL 路径必须是 {expected_path}"));
    }
    Ok(())
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
    /// Reject settings that could send the bearer token to an unexpected host.
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != "anthropic" && self.protocol != "openai" {
            return Err("协议必须是 anthropic 或 openai".into());
        }

        let expected_path = if self.is_anthropic() {
            "/api/coding"
        } else {
            "/api/coding/v3"
        };
        validate_official_ark_base_url(&self.base_url, expected_path)?;

        let model = self.model.trim();
        if model.is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
            return Err("模型名称无效".into());
        }
        if !self.temperature.is_finite() || !(0.0..=2.0).contains(&self.temperature) {
            return Err("temperature 必须在 0 到 2 之间".into());
        }
        if !(1..=65_536).contains(&self.max_tokens) {
            return Err("max_tokens 必须在 1 到 65536 之间".into());
        }
        if !(1..=16).contains(&self.max_concurrency) {
            return Err("最大并发必须在 1 到 16 之间".into());
        }
        if !(5..=3_600).contains(&self.timeout_secs) {
            return Err("超时必须在 5 到 3600 秒之间".into());
        }
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_protocol_matching_official_ark_urls() {
        let settings = ArkSettings::default();
        assert!(settings.validate().is_ok());

        let mut openai = settings.clone();
        openai.protocol = "openai".into();
        openai.base_url = "https://ark.cn-beijing.volces.com/api/coding/v3".into();
        assert!(openai.validate().is_ok());

        for unsafe_url in [
            "http://ark.cn-beijing.volces.com/api/coding",
            "https://127.0.0.1/api/coding",
            "https://ark.cn-beijing.volces.com.evil.example/api/coding",
            "https://user:pass@ark.cn-beijing.volces.com/api/coding",
            "https://ark.cn-beijing.volces.com:8443/api/coding",
            "https://ark.cn-beijing.volces.com/api/coding?next=evil",
        ] {
            let mut candidate = settings.clone();
            candidate.base_url = unsafe_url.into();
            assert!(candidate.validate().is_err(), "accepted {unsafe_url}");
        }
    }

    #[test]
    fn serialized_settings_never_include_api_key() {
        let settings = ArkSettings {
            api_key: "secret-value".into(),
            ..ArkSettings::default()
        };
        let value = serde_json::to_value(settings).unwrap();
        assert!(value.get("api_key").is_none());
        assert!(!value.to_string().contains("secret-value"));
    }
}
