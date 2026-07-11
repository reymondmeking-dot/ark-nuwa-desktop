//! Distillation kernel: bundles the built-in nuwa workflow and helpers to
//! extract the final SKILL.md artifact from a completed run's context.

use crate::workflow::{Context, WorkflowSpec};

/// The built-in nuwa distillation workflow, embedded at compile time so the app
/// always ships with a working default even before the user writes their own.
pub const NUWA_DISTILL_YAML: &str = include_str!("../../workflows/nuwa-distill.yaml");

/// Parse the built-in workflow.
pub fn default_workflow() -> WorkflowSpec {
    WorkflowSpec::parse(NUWA_DISTILL_YAML).expect("built-in nuwa-distill.yaml must be valid")
}

/// After a distillation run, pull out the finished SKILL.md text. Prefers the
/// post-review `final_skill`, falling back to the first-pass `skill`.
pub fn extract_skill(ctx: &Context) -> Option<String> {
    ctx.output("final_skill")
        .or_else(|| ctx.output("skill"))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Engine;

    #[test]
    fn builtin_workflow_is_valid() {
        let spec = default_workflow();
        // Plan must succeed: no cycles, deps resolve, goto is an ancestor.
        let layers = Engine::plan(&spec).expect("builtin must plan cleanly");
        // The 6 research nodes have no deps -> they share the first layer.
        assert!(
            layers[0].len() >= 6,
            "research tracks should run in parallel"
        );
    }
}
