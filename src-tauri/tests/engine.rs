//! Integration tests for the workflow DAG engine, exercised with a deterministic
//! MockClient — no network. Covers: parallel layering, template data flow,
//! cycle rejection, bad-goto rejection, and the closed-loop validation retry.

use ark_nuwa_lib::mock::MockClient;
use ark_nuwa_lib::workflow::{Engine, RunEvent, SpecError, WorkflowSpec};
use std::sync::{Arc, Mutex};

/// Collect every emitted event into a shared vec for assertions.
fn recorder() -> (ark_nuwa_lib::workflow::EventSink, Arc<Mutex<Vec<RunEvent>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let log2 = log.clone();
    let sink: ark_nuwa_lib::workflow::EventSink =
        Arc::new(move |ev: RunEvent| log2.lock().unwrap().push(ev));
    (sink, log)
}

#[tokio::test]
async fn parallel_layer_and_dataflow() {
    let yaml = r#"
name: t
vars: { topic: "X" }
nodes:
  - { id: a, type: llm, prompt: "研究A {{topic}}", output: a }
  - { id: b, type: llm, prompt: "研究B {{topic}}", output: b }
  - id: merge
    type: synthesize
    depends_on: [a, b]
    prompt: "合并 {{a}} 和 {{b}}"
    output: merged
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();
    let layers = Engine::plan(&spec).unwrap();
    // a and b have no deps -> same first layer; merge in the second.
    assert_eq!(layers[0], vec!["a".to_string(), "b".to_string()]);
    assert_eq!(layers[1], vec!["merge".to_string()]);

    let mock = MockClient::new();
    mock.on("研究A", &["AAA"])
        .on("研究B", &["BBB"])
        .on("合并", &["MERGED:AAA+BBB"]);

    let engine = Engine::new(Arc::new(mock), "ep-test".into());
    let (sink, _log) = recorder();
    let ctx = engine.run(&spec, sink).await.unwrap();

    assert_eq!(ctx.output("a"), Some("AAA"));
    assert_eq!(ctx.output("merged"), Some("MERGED:AAA+BBB"));
}

#[tokio::test]
async fn stores_outputs_by_node_id_and_output_key() {
    let yaml = r#"
name: aliases
nodes:
  - id: synth
    type: synthesize
    prompt: "make synth"
    output: framework
  - id: gate
    type: validate
    depends_on: [synth]
    criteria: ["material must be present"]
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();

    let mock = MockClient::new();
    mock.on("make synth", &["MATERIAL_FROM_SYNTH"])
        // This only matches if validate receives synth's material through its
        // depends_on node id. Before the alias fix, the material was empty.
        .on("MATERIAL_FROM_SYNTH", &["PASS material received"]);

    let engine = Engine::new(Arc::new(mock), "ep".into());
    let (sink, _log) = recorder();
    let ctx = engine.run(&spec, sink).await.unwrap();

    assert_eq!(ctx.output("framework"), Some("MATERIAL_FROM_SYNTH"));
    assert_eq!(ctx.output("synth"), Some("MATERIAL_FROM_SYNTH"));
    assert_eq!(ctx.output("gate"), Some("PASS material received"));
}

#[tokio::test]
async fn rejects_cycle() {
    let yaml = r#"
name: c
nodes:
  - { id: a, type: llm, prompt: "x", depends_on: [b] }
  - { id: b, type: llm, prompt: "y", depends_on: [a] }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();
    let err = Engine::plan(&spec).unwrap_err();
    assert!(matches!(err, SpecError::Cycle(_)), "got {err:?}");
}

#[tokio::test]
async fn rejects_unknown_dependency() {
    let yaml = r#"
name: u
nodes:
  - { id: a, type: llm, prompt: "x", depends_on: [ghost] }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();
    let err = Engine::plan(&spec).unwrap_err();
    assert!(
        matches!(err, SpecError::UnknownDependency { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn rejects_goto_that_is_not_ancestor() {
    // validate's goto points to a node that is NOT upstream of it.
    let yaml = r#"
name: g
nodes:
  - { id: a, type: llm, prompt: "x", output: a }
  - { id: other, type: llm, prompt: "y", output: other }
  - id: v
    type: validate
    depends_on: [a]
    criteria: ["ok"]
    on_fail: { goto: other, max: 1 }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();
    let err = Engine::plan(&spec).unwrap_err();
    assert!(matches!(err, SpecError::BadGoto { .. }), "got {err:?}");
}

#[tokio::test]
async fn closed_loop_retries_then_passes() {
    // synth -> validate. Validate FAILs first, loops back to synth, then PASSes.
    let yaml = r#"
name: loop
vars: { person: "P" }
nodes:
  - id: synth
    type: synthesize
    prompt: "合成框架 {{person}}"
    output: framework
  - id: gate
    type: validate
    depends_on: [synth]
    criteria: ["有区分度"]
    on_fail: { goto: synth, max: 2 }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();

    let mock = MockClient::new();
    // synth called twice (initial + 1 retry); cycling responses.
    mock.on("合成框架", &["草稿v1", "草稿v2"]);
    // validate: first FAIL, then PASS.
    mock.on("有区分度", &["FAIL 不够独特", "PASS 合格"]);

    let engine = Engine::new(Arc::new(mock), "ep".into());
    let (sink, log) = recorder();
    let ctx = engine.run(&spec, sink).await.unwrap();

    // Final framework is the retried version.
    assert_eq!(ctx.output("framework"), Some("草稿v2"));

    // A LoopBack event must have been emitted exactly once.
    let loopbacks = log
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, RunEvent::LoopBack { .. }))
        .count();
    assert_eq!(loopbacks, 1, "expected one loop-back");

    // Final event is success.
    assert!(matches!(
        log.lock().unwrap().last().unwrap(),
        RunEvent::Finished { ok: true }
    ));
}

#[tokio::test]
async fn retry_prompt_can_use_previous_validation_feedback() {
    let yaml = r#"
name: feedback
nodes:
  - id: synth
    type: synthesize
    prompt: "合成。上一轮反馈：{{?validation}}"
    output: framework
  - id: gate
    type: validate
    depends_on: [synth]
    criteria: ["有区分度"]
    output: validation
    on_fail: { goto: synth, max: 2 }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();

    let mock = MockClient::new();
    mock.on("缺乏区分度", &["草稿v2: 更独特"])
        .on("合成。上一轮反馈：", &["草稿v1"])
        .on("有区分度", &["FAIL 缺乏区分度", "PASS 合格"]);

    let engine = Engine::new(Arc::new(mock), "ep".into());
    let (sink, _log) = recorder();
    let ctx = engine.run(&spec, sink).await.unwrap();

    assert_eq!(ctx.output("framework"), Some("草稿v2: 更独特"));
    assert_eq!(ctx.output("validation"), Some("PASS 合格"));
}

#[tokio::test]
async fn closed_loop_fails_after_max_attempts() {
    let yaml = r#"
name: loopfail
nodes:
  - { id: synth, type: synthesize, prompt: "合成", output: framework }
  - id: gate
    type: validate
    depends_on: [synth]
    criteria: ["ok"]
    on_fail: { goto: synth, max: 2 }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();

    let mock = MockClient::new();
    mock.on("合成", &["draft"]);
    // Always FAIL -> should exhaust max (2) and error out.
    mock.on("ok", &["FAIL 永远不通过"]);

    let engine = Engine::new(Arc::new(mock), "ep".into());
    let (sink, log) = recorder();
    let result = engine.run(&spec, sink).await;

    assert!(result.is_err(), "run should fail after exhausting retries");
    // Last event must report failure.
    assert!(matches!(
        log.lock().unwrap().last().unwrap(),
        RunEvent::Finished { ok: false }
    ));
}

#[tokio::test]
async fn missing_template_key_errors() {
    let yaml = r#"
name: miss
nodes:
  - { id: a, type: llm, prompt: "用了 {{nonexistent}}", output: a }
"#;
    let spec = WorkflowSpec::parse(yaml).unwrap();
    let mock = MockClient::new();
    mock.default_reply("x");
    let engine = Engine::new(Arc::new(mock), "ep".into());
    let (sink, _log) = recorder();
    let err = engine.run(&spec, sink).await.unwrap_err();
    assert!(err.to_string().contains("nonexistent"), "got {err}");
}
