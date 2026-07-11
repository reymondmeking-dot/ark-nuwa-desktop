//! DAG execution engine.
//!
//! Responsibilities:
//!   - validate the spec (unknown deps, cycles, illegal `on_fail.goto`)
//!   - topological layering so independent nodes run concurrently
//!   - bounded parallelism within a layer
//!   - closed-loop retry: a `Validate` node that judges FAIL rewinds execution
//!     to its `on_fail.goto` ancestor and re-runs, bounded by `max`
//!   - per-node status events for the UI

use super::context::Context;
use super::model::{Node, NodeType, SpecError, WorkflowSpec};
use crate::llm::LlmClient;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

/// Lifecycle of a single node, surfaced to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Running,
    Done,
    Failed,
    Retrying,
    Skipped,
}

/// An event emitted during execution. The Tauri layer forwards these to the
/// frontend; tests collect them into a vec.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    NodeStatus {
        node: String,
        status: NodeStatus,
    },
    NodeChunk {
        node: String,
        delta: String,
    },
    NodeOutput {
        node: String,
        output: String,
    },
    LoopBack {
        from: String,
        to: String,
        attempt: u32,
    },
    Log {
        message: String,
    },
    Finished {
        ok: bool,
    },
}

/// Sink for run events. `Arc<dyn>` so it can be cloned across async tasks.
pub type EventSink = Arc<dyn Fn(RunEvent) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Spec(#[from] SpecError),
    #[error("loop limit exceeded at validation node '{0}'")]
    LoopLimit(String),
    #[error("missing template keys in node '{node}': {keys:?}")]
    MissingKeys { node: String, keys: Vec<String> },
    #[error("llm error in node '{node}': {source}")]
    Llm {
        node: String,
        source: crate::llm::LlmError,
    },
}

pub struct Engine {
    client: Arc<dyn LlmClient>,
    model: String,
    max_tokens: u32,
}

fn store_node_output(ctx: &mut Context, node: &Node, value: String) {
    ctx.set_output(node.output_key(), value.clone());
    if node.output_key() != node.id {
        ctx.set_output(&node.id, value);
    }
}

impl Engine {
    pub fn new(client: Arc<dyn LlmClient>, model: String) -> Self {
        Self {
            client,
            model,
            max_tokens: 8192,
        }
    }

    /// Set the per-node output token limit (from user settings). Clamped to a
    /// sane single-response ceiling — users sometimes enter the context-window
    /// size here, which the gateway rejects.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        if max_tokens > 0 {
            self.max_tokens = max_tokens.min(65536);
        }
        self
    }

    /// Validate structure and return topological layers (each layer is a set of
    /// node ids with no inter-dependencies — safe to run concurrently).
    pub fn plan(spec: &WorkflowSpec) -> Result<Vec<Vec<String>>, SpecError> {
        if spec.nodes.is_empty() {
            return Err(SpecError::Empty);
        }
        // Unique ids.
        let mut ids = BTreeSet::new();
        for n in &spec.nodes {
            if !ids.insert(n.id.clone()) {
                return Err(SpecError::DuplicateId(n.id.clone()));
            }
        }
        // Known dependencies.
        for n in &spec.nodes {
            for d in &n.depends_on {
                if !ids.contains(d) {
                    return Err(SpecError::UnknownDependency {
                        node: n.id.clone(),
                        dep: d.clone(),
                    });
                }
            }
        }

        // Kahn's algorithm for topological layering + cycle detection.
        let mut indegree: BTreeMap<String, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for n in &spec.nodes {
            indegree.entry(n.id.clone()).or_insert(0);
            for d in &n.depends_on {
                *indegree.entry(n.id.clone()).or_insert(0) += 1;
                dependents.entry(d.clone()).or_default().push(n.id.clone());
            }
        }

        let mut layers: Vec<Vec<String>> = Vec::new();
        let mut frontier: VecDeque<String> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut processed = 0usize;

        while !frontier.is_empty() {
            let mut layer: Vec<String> = frontier.iter().cloned().collect();
            layer.sort();
            frontier.clear();
            for id in &layer {
                processed += 1;
                if let Some(children) = dependents.get(id) {
                    for c in children {
                        let e = indegree.get_mut(c).unwrap();
                        *e -= 1;
                        if *e == 0 {
                            frontier.push_back(c.clone());
                        }
                    }
                }
            }
            layers.push(layer);
        }

        if processed != spec.nodes.len() {
            // Remaining nodes with nonzero indegree form a cycle.
            let stuck: Vec<String> = indegree
                .iter()
                .filter(|(_, &d)| d > 0)
                .map(|(k, _)| k.clone())
                .collect();
            return Err(SpecError::Cycle(stuck.join(", ")));
        }

        // Validate that every on_fail.goto is a true ancestor of the node.
        for n in &spec.nodes {
            if let Some(of) = &n.on_fail {
                if !is_ancestor(spec, &of.goto, &n.id) {
                    return Err(SpecError::BadGoto {
                        node: n.id.clone(),
                        goto: of.goto.clone(),
                    });
                }
            }
        }

        Ok(layers)
    }

    /// Execute the workflow to completion (or failure). Returns the final
    /// context (all node outputs) on success.
    pub async fn run(&self, spec: &WorkflowSpec, emit: EventSink) -> Result<Context, RunError> {
        let layers = Self::plan(spec)?;
        let mut ctx = Context::new(spec.vars.clone());

        // Track attempts per validate node to enforce loop bounds.
        let mut attempts: BTreeMap<String, u32> = BTreeMap::new();
        // Flat execution order (layer by layer); we may rewind within it.
        let order: Vec<String> = layers.iter().flatten().cloned().collect();

        let mut i = 0usize;
        // Group consecutive nodes by layer for concurrent execution, but allow
        // rewinding to an arbitrary index for the closed loop.
        let layer_of = layer_index_map(&layers);

        while i < order.len() {
            // Determine the current layer slice starting at i.
            let cur_layer = layer_of[&order[i]];
            let mut batch: Vec<String> = Vec::new();
            let mut j = i;
            while j < order.len() && layer_of[&order[j]] == cur_layer {
                batch.push(order[j].clone());
                j += 1;
            }

            // Run the batch concurrently (bounded by max_concurrency).
            let rewind = self
                .run_batch(spec, &batch, &mut ctx, &mut attempts, &emit)
                .await?;

            if let Some(goto) = rewind {
                // Closed loop: jump back to the goto node's index and re-run
                // from there. Outputs downstream of goto are recomputed.
                let target = order.iter().position(|x| x == &goto).unwrap();
                i = target;
            } else {
                i = j;
            }
        }

        emit(RunEvent::Finished { ok: true });
        Ok(ctx)
    }

    /// Run one layer's nodes concurrently. Returns `Some(goto)` if a validation
    /// node failed and requests a loop-back.
    async fn run_batch(
        &self,
        spec: &WorkflowSpec,
        batch: &[String],
        ctx: &mut Context,
        attempts: &mut BTreeMap<String, u32>,
        emit: &EventSink,
    ) -> Result<Option<String>, RunError> {
        use futures_util::stream::{self, StreamExt};

        // Validation nodes are evaluated after the parallel batch so their
        // loop-back decision is deterministic. Split them out. Owned ids so the
        // concurrent async blocks below carry no borrowed (higher-ranked)
        // lifetime — which `tauri::generate_handler` cannot prove general enough.
        let (validators, workers): (Vec<String>, Vec<String>) = batch
            .iter()
            .cloned()
            .partition(|id| spec.node(id).unwrap().node_type == NodeType::Validate);

        // Execute non-validator nodes concurrently.
        let results: Vec<(String, Result<String, RunError>)> = stream::iter(workers)
            .map(|id| {
                let node = spec.node(&id).unwrap().clone();
                let snapshot = ctx.clone();
                let emit = emit.clone();
                let client = self.client.clone();
                let model = self.model.clone();
                let max_tokens = self.max_tokens;
                async move {
                    let out =
                        execute_node(&node, &snapshot, client, &model, max_tokens, &emit).await;
                    (node.id.clone(), out)
                }
            })
            .buffer_unordered(spec.max_concurrency.max(1))
            .collect()
            .await;

        for (id, res) in results {
            let node = spec.node(&id).unwrap();
            match res {
                Ok(out) => {
                    store_node_output(ctx, node, out.clone());
                    emit(RunEvent::NodeOutput {
                        node: id.clone(),
                        output: out,
                    });
                    emit(RunEvent::NodeStatus {
                        node: id,
                        status: NodeStatus::Done,
                    });
                }
                Err(e) => {
                    emit(RunEvent::NodeStatus {
                        node: id,
                        status: NodeStatus::Failed,
                    });
                    return Err(e);
                }
            }
        }

        // Now evaluate validators sequentially (cheap; deterministic loop).
        for id in validators {
            let node = spec.node(&id).unwrap().clone();
            emit(RunEvent::NodeStatus {
                node: id.clone(),
                status: NodeStatus::Running,
            });
            let verdict = execute_node(
                &node,
                ctx,
                self.client.clone(),
                &self.model,
                self.max_tokens,
                emit,
            )
            .await?;
            store_node_output(ctx, &node, verdict.clone());
            let passed = verdict_passed(&verdict);

            if passed {
                emit(RunEvent::NodeOutput {
                    node: id.clone(),
                    output: verdict,
                });
                emit(RunEvent::NodeStatus {
                    node: id.clone(),
                    status: NodeStatus::Done,
                });
            } else if let Some(of) = &node.on_fail {
                let n = attempts.entry(id.clone()).or_insert(0);
                *n += 1;
                if *n > of.max {
                    emit(RunEvent::NodeStatus {
                        node: id.clone(),
                        status: NodeStatus::Failed,
                    });
                    emit(RunEvent::Finished { ok: false });
                    return Err(RunError::LoopLimit(id.clone()));
                }
                emit(RunEvent::LoopBack {
                    from: id.clone(),
                    to: of.goto.clone(),
                    attempt: *n,
                });
                emit(RunEvent::NodeStatus {
                    node: id.clone(),
                    status: NodeStatus::Retrying,
                });
                return Ok(Some(of.goto.clone()));
            } else {
                // No loop-back configured: a failed gate fails the run.
                emit(RunEvent::NodeStatus {
                    node: id.clone(),
                    status: NodeStatus::Failed,
                });
                emit(RunEvent::Finished { ok: false });
                return Err(RunError::LoopLimit(id.clone()));
            }
        }

        Ok(None)
    }
}

/// Build a node-id -> layer-index map for grouping.
fn layer_index_map(layers: &[Vec<String>]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for (i, layer) in layers.iter().enumerate() {
        for id in layer {
            m.insert(id.clone(), i);
        }
    }
    m
}

/// Is `ancestor` reachable upstream from `node` via depends_on edges?
fn is_ancestor(spec: &WorkflowSpec, ancestor: &str, node: &str) -> bool {
    let mut stack = vec![node.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(cur) = stack.pop() {
        let Some(n) = spec.node(&cur) else { continue };
        for d in &n.depends_on {
            if d == ancestor {
                return true;
            }
            if seen.insert(d.clone()) {
                stack.push(d.clone());
            }
        }
    }
    false
}

/// Heuristic verdict parser: a validation reply passes unless it clearly says
/// it failed. We look for explicit PASS/FAIL markers first, then Chinese cues.
fn verdict_passed(text: &str) -> bool {
    let upper = text.to_uppercase();
    if upper.contains("FAIL") || text.contains("不通过") || text.contains("未通过") {
        return false;
    }
    if upper.contains("PASS") || text.contains("通过") {
        return true;
    }
    // Default to pass if the model gave substantive content without a fail cue.
    !text.trim().is_empty()
}

/// Execute a single node against an immutable context snapshot.
async fn execute_node(
    node: &Node,
    ctx: &Context,
    client: Arc<dyn LlmClient>,
    model: &str,
    max_tokens: u32,
    emit: &EventSink,
) -> Result<String, RunError> {
    use crate::llm::{ChatMessage, ChatRequest};

    emit(RunEvent::NodeStatus {
        node: node.id.clone(),
        status: NodeStatus::Running,
    });

    // Passthrough merges upstream outputs without calling the model.
    if node.node_type == NodeType::Passthrough {
        let merged = node
            .depends_on
            .iter()
            .filter_map(|d| ctx.output(d).map(|v| format!("## {d}\n{v}")))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok(merged);
    }

    // Build the prompt: validate nodes synthesize one from criteria.
    let template = match node.node_type {
        NodeType::Validate => build_validate_prompt(node, ctx),
        _ => node.prompt.clone().unwrap_or_default(),
    };

    let (prompt, missing) = ctx.render(&template);
    if !missing.is_empty() {
        return Err(RunError::MissingKeys {
            node: node.id.clone(),
            keys: missing,
        });
    }

    let mut req = ChatRequest::new(model.to_string(), vec![ChatMessage::user(prompt)]);
    req.max_tokens = Some(max_tokens);

    // Stream so the UI sees live tokens; concatenated text is the output.
    let node_id = node.id.clone();
    let emit2 = emit.clone();
    let mut sink = move |delta: String| {
        emit2(RunEvent::NodeChunk {
            node: node_id.clone(),
            delta,
        });
    };
    client
        .stream(&req, &mut sink)
        .await
        .map_err(|source| RunError::Llm {
            node: node.id.clone(),
            source,
        })
}

/// Compose a judge prompt for a validation gate from its criteria, embedding
/// the upstream material to be judged.
fn build_validate_prompt(node: &Node, ctx: &Context) -> String {
    let material = node
        .depends_on
        .iter()
        .filter_map(|d| ctx.output(d).map(|v| format!("【{d}】\n{v}")))
        .collect::<Vec<_>>()
        .join("\n\n");
    let criteria = node
        .criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {c}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    // If the node also has a custom prompt, prefer it (still gets material).
    if let Some(p) = &node.prompt {
        return format!("{p}\n\n待审材料：\n{material}");
    }

    format!(
        "你是严格的质量审查员。请依据以下标准审查材料是否合格：\n{criteria}\n\n\
         待审材料：\n{material}\n\n\
         若全部满足，请在首行输出 PASS 并简述理由；\
         若任一不满足，请在首行输出 FAIL 并指出缺陷。"
    )
}
