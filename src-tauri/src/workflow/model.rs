//! Workflow specification — the data model behind the YAML/JSON DAG.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a node does when executed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// Plain LLM call: render `prompt`, send to Ark, store the reply.
    Llm,
    /// Synthesis: same as `llm` but conventionally fans in many upstream
    /// outputs to build the cognitive framework.
    Synthesize,
    /// Validation gate: render `prompt` (built from `criteria`), ask the model
    /// to judge PASS/FAIL. On FAIL, follows `on_fail` to close the loop.
    Validate,
    /// Generate the final SKILL.md artifact from upstream context.
    GenerateSkill,
    /// Quality test: sanity / edge-case / voice checks against the skill.
    Test,
    /// No LLM call; just merges/forwards upstream outputs (checkpoints).
    Passthrough,
}

/// Action taken when a `Validate` node judges FAIL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnFail {
    /// Node id to jump back to and re-run (must be an ancestor).
    pub goto: String,
    /// Maximum number of loop-back attempts before the workflow fails.
    #[serde(default = "default_max")]
    pub max: u32,
}

fn default_max() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    /// Display label (optional, falls back to id).
    #[serde(default)]
    pub label: Option<String>,
    /// Prompt template with `{{var}}` / `{{node_output}}` placeholders.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Validation criteria (used by `Validate` to build its judge prompt).
    #[serde(default)]
    pub criteria: Vec<String>,
    /// Upstream node ids this node depends on.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Context key under which this node's output is stored. Defaults to id.
    #[serde(default)]
    pub output: Option<String>,
    /// Loop-back behaviour for validation gates.
    #[serde(default)]
    pub on_fail: Option<OnFail>,
}

impl Node {
    pub fn output_key(&self) -> &str {
        self.output.as_deref().unwrap_or(&self.id)
    }
    pub fn display(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Global variables available to every prompt as `{{key}}`.
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    pub nodes: Vec<Node>,
    /// Max nodes run concurrently within a topological layer.
    #[serde(default = "default_concurrency")]
    pub max_concurrency: usize,
}

fn default_concurrency() -> usize {
    6
}

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("duplicate node id: {0}")]
    DuplicateId(String),
    #[error("node '{node}' depends on unknown node '{dep}'")]
    UnknownDependency { node: String, dep: String },
    #[error("workflow has a cycle involving: {0}")]
    Cycle(String),
    #[error("on_fail.goto '{goto}' on node '{node}' is not an ancestor")]
    BadGoto { node: String, goto: String },
    #[error("empty workflow: no nodes")]
    Empty,
}

impl WorkflowSpec {
    pub fn from_yaml(s: &str) -> Result<Self, SpecError> {
        serde_yaml::from_str(s).map_err(|e| SpecError::Parse(e.to_string()))
    }

    pub fn from_json(s: &str) -> Result<Self, SpecError> {
        serde_json::from_str(s).map_err(|e| SpecError::Parse(e.to_string()))
    }

    /// Accept either YAML or JSON (JSON is a subset of YAML, but we try JSON
    /// first for clearer errors when the source is obviously JSON).
    pub fn parse(s: &str) -> Result<Self, SpecError> {
        let trimmed = s.trim_start();
        if trimmed.starts_with('{') {
            Self::from_json(s)
        } else {
            Self::from_yaml(s)
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}
