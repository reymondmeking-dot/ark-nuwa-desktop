//! End-to-end test: run the built-in nuwa distillation workflow with a
//! MockClient and assert the full closed loop produces a SKILL and exercises
//! the quality-test nodes.

use ark_nuwa_lib::distill;
use ark_nuwa_lib::mock::MockClient;
use ark_nuwa_lib::workflow::{Engine, RunEvent};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn full_nuwa_distillation_produces_skill() {
    let spec = distill::default_workflow();

    // Mock keyed on distinctive phrases in each node's prompt.
    let mock = MockClient::new();
    mock.on("著作 / 长文", &["著作要点"])
        .on("访谈 / 播客", &["访谈要点"])
        .on("表达 DNA", &["风格要点"])
        .on("评价与批评", &["外部评价"])
        .on("关键决策史", &["决策要点"])
        .on("人生/事业时间线", &["时间线要点"])
        .on("提炼", &["心智模型X（跨领域、可预测、有区分度）"]) // synthesize_framework
        .on("质量审查员", &["PASS 框架合格"]) // validate gate
        .on(
            "写成一个可运行的视角 Skill",
            &["---\nname: P-perspective\n---\n# skill"],
        ) // generate
        .on("公开讨论过", &["一致"]) // test_sanity
        .on("没有直接讨论过", &["恰当表达了不确定"]) // test_edge
        .on("表达 DNA（句式", &["风格吻合"]) // test_voice
        .on("批判性评审员", &["建议：保持诚实边界"]) // review
        .on(
            "输出最终版 Skill",
            &["---\nname: P-perspective\n---\n# 最终 skill"],
        ) // finalize
        .default_reply("（默认回复）");

    let engine = Engine::new(Arc::new(mock), "ep-test".into());

    let log = Arc::new(Mutex::new(Vec::new()));
    let log2 = log.clone();
    let sink: ark_nuwa_lib::workflow::EventSink =
        Arc::new(move |ev: RunEvent| log2.lock().unwrap().push(ev));

    let ctx = engine.run(&spec, sink).await.expect("distillation run");

    // Final skill is produced (post-review version preferred).
    let skill = distill::extract_skill(&ctx).expect("a skill should be produced");
    assert!(skill.contains("最终 skill"));

    // Quality-test nodes executed and stored outputs.
    assert!(ctx.output("test_sanity").is_some());
    assert!(ctx.output("test_edge").is_some());
    assert!(ctx.output("test_voice").is_some());

    // Run finished successfully.
    assert!(matches!(
        log.lock().unwrap().last().unwrap(),
        RunEvent::Finished { ok: true }
    ));
}
