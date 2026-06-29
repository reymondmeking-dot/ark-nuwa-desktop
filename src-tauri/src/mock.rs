//! Deterministic mock LLM client for tests.
//!
//! Matches incoming requests against registered rules (by substring of the
//! last user/system message) and returns scripted responses. This lets the DAG
//! engine and the full distillation loop run under `cargo test` with no network.

use crate::llm::{ChatRequest, ChunkSink, LlmClient, LlmError};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct Rule {
    needle: String,
    responses: Vec<String>,
    cursor: AtomicUsize,
}

#[derive(Default)]
pub struct MockClient {
    rules: Mutex<Vec<Rule>>,
    default: Mutex<Option<String>>,
    pub calls: AtomicUsize,
}

impl MockClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// When a request's combined prompt contains `needle`, reply with the next
    /// response in `responses` (cycling to successive ones on repeat hits — so
    /// a validate→retry loop can fail first, then pass).
    pub fn on(&self, needle: &str, responses: &[&str]) -> &Self {
        self.rules.lock().unwrap().push(Rule {
            needle: needle.to_string(),
            responses: responses.iter().map(|s| s.to_string()).collect(),
            cursor: AtomicUsize::new(0),
        });
        self
    }

    /// Fallback response for any unmatched request.
    pub fn default_reply(&self, text: &str) -> &Self {
        *self.default.lock().unwrap() = Some(text.to_string());
        self
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn resolve(&self, req: &ChatRequest) -> Result<String, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let haystack: String = req
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let rules = self.rules.lock().unwrap();
        for rule in rules.iter() {
            if haystack.contains(&rule.needle) {
                let idx = rule.cursor.fetch_add(1, Ordering::SeqCst);
                let i = idx.min(rule.responses.len() - 1);
                return Ok(rule.responses[i].clone());
            }
        }
        drop(rules);

        if let Some(d) = self.default.lock().unwrap().clone() {
            return Ok(d);
        }
        Err(LlmError::Decode(format!(
            "MockClient: no rule matched prompt: {}",
            &haystack.chars().take(80).collect::<String>()
        )))
    }
}

#[async_trait]
impl LlmClient for MockClient {
    async fn complete(&self, req: &ChatRequest) -> Result<String, LlmError> {
        self.resolve(req)
    }

    async fn stream(
        &self,
        req: &ChatRequest,
        on_chunk: &mut ChunkSink<'_>,
    ) -> Result<String, LlmError> {
        let text = self.resolve(req)?;
        // Emit a couple of chunks so streaming consumers are exercised.
        for piece in text.split_inclusive(' ') {
            on_chunk(piece.to_string());
        }
        Ok(text)
    }
}
