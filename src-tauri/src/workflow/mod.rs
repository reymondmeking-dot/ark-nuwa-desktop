//! Workflow subsystem: spec model, execution context, and the DAG engine.

pub mod context;
pub mod engine;
pub mod model;

pub use context::Context;
pub use engine::{Engine, EventSink, NodeStatus, RunError, RunEvent};
pub use model::{Node, NodeType, OnFail, SpecError, WorkflowSpec};
