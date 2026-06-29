//! Tauri command surface exposed to the frontend.

use crate::anthropic::AnthropicClient;
use crate::ark::ArkClient;
use crate::config::ArkSettings;
use crate::distill;
use crate::llm::{ChatMessage, ChatRequest, LlmClient};
use crate::session::ChatSession;
use crate::workflow::{Engine, RunEvent, WorkflowSpec};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

/// Shared app state.
#[derive(Default)]
pub struct AppState {
    pub settings: Mutex<ArkSettings>,
    pub sessions: Mutex<Vec<ChatSession>>,
}

const STORE_FILE: &str = "settings.json";

/// Build the right LLM client for the configured protocol. The Anthropic
/// endpoint is where the console-managed `ark-code-latest` alias resolves.
fn build_client(settings: &ArkSettings) -> Result<Arc<dyn LlmClient>, String> {
    let cfg = settings.to_ark_config();
    if settings.is_anthropic() {
        Ok(Arc::new(AnthropicClient::new(cfg).map_err(|e| e.to_string())?))
    } else {
        Ok(Arc::new(ArkClient::new(cfg).map_err(|e| e.to_string())?))
    }
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> serde_json::Value {
    state.settings.lock().unwrap().redacted()
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    state: State<AppState>,
    settings: ArkSettings,
) -> Result<(), String> {
    {
        let mut guard = state.settings.lock().unwrap();
        // Preserve an existing key if the UI sent an empty one (means "unchanged").
        let mut incoming = settings;
        if incoming.api_key.trim().is_empty() {
            incoming.api_key = guard.api_key.clone();
        }
        *guard = incoming;
    }
    persist(&app, &state).map_err(|e| e.to_string())
}

/// Send one minimal request to verify the Ark connection works.
#[tauri::command]
pub async fn test_connection(state: State<'_, AppState>) -> Result<String, String> {
    let (settings, model, base_url, is_anthropic) = {
        let s = state.settings.lock().unwrap();
        (s.clone(), s.model.clone(), s.base_url.clone(), s.is_anthropic())
    };
    let client = build_client(&settings)?;
    let mut req = ChatRequest::new(
        String::new(),
        vec![ChatMessage::user("ping，请只回复 pong")],
    );
    req.max_tokens = Some(32);
    match client.complete(&req).await {
        Ok(r) => Ok(format!(
            "连接成功（{}协议）：{}",
            if is_anthropic { "Anthropic" } else { "OpenAI" },
            r.chars().take(60).collect::<String>()
        )),
        Err(e) => {
            let mut msg = format!("连接失败：{e}");
            let is_404 = e.to_string().contains("404");
            if is_404 {
                msg.push_str("\n\n可能原因（Coding Plan 常见 404）：");
                if model == "ark-code-latest" && !is_anthropic {
                    msg.push_str(
                        "\n• 你用的是 OpenAI 协议口（/api/coding/v3）+ ark-code-latest —— 这个托管别名在 OpenAI 口解析不到。\
                         请在「协议」里改用 Anthropic（这是 Claude Code 用的口，ark-code-latest 在此可用），或改用具体模型名。",
                    );
                }
                if is_anthropic && !base_url.contains("/api/coding") {
                    msg.push_str(&format!(
                        "\n• Anthropic 协议的 base_url 应为 https://ark.cn-beijing.volces.com/api/coding（当前：{base_url}）。",
                    ));
                }
                if !is_anthropic && !base_url.contains("/api/coding") {
                    msg.push_str(&format!(
                        "\n• OpenAI 协议的 base_url 应为 https://ark.cn-beijing.volces.com/api/coding/v3（当前：{base_url}）。",
                    ));
                }
                msg.push_str(
                    "\n• 确认该 API Key 已订阅 Coding Plan 套餐，且控制台「开通管理」已选定生效模型。",
                );
            }
            Err(msg)
        }
    }
}

// ---------- workflow ----------

#[tauri::command]
pub fn default_workflow_yaml() -> String {
    distill::NUWA_DISTILL_YAML.to_string()
}

/// Known coding-plan model names (from Ark docs) plus the console-managed
/// `ark-code-latest`. Used to populate the settings dropdown so users don't
/// mistype. `ark-code-latest` follows whatever model is selected in the Ark
/// console "开通管理" page.
#[tauri::command]
pub fn coding_models() -> Vec<&'static str> {
    vec![
        "ark-code-latest",
        "doubao-seed-2.0-code",
        "doubao-seed-2.0-pro",
        "doubao-seed-2.0-lite",
        "doubao-seed-code",
        "minimax-m2.7",
        "minimax-m3",
        "glm-5.2",
        "deepseek-v4-flash",
        "deepseek-v4-pro",
        "kimi-k2.6",
        "kimi-k2.7-code",
    ]
}

/// Validate a workflow spec and return its topological layers (for DAG preview)
/// or a structured error.
#[tauri::command]
pub fn validate_workflow(source: String) -> Result<serde_json::Value, String> {
    let spec = WorkflowSpec::parse(&source).map_err(|e| e.to_string())?;
    let layers = Engine::plan(&spec).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "name": spec.name,
        "node_count": spec.nodes.len(),
        "layers": layers,
    }))
}

// ---------- workflow library (save/load to a workflows/ directory) ----------

/// Directory where user workflows live: `<app_data_dir>/workflows`. Works both
/// in dev and in a packaged app. Created on demand.
fn workflows_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录：{e}"))?;
    let dir = base.join("workflows");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 workflows 目录失败：{e}"))?;
    Ok(dir)
}

/// Sanitize a name into a safe `.yaml` filename (no path traversal).
fn safe_filename(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stem = stem.trim_matches('_');
    let stem = if stem.is_empty() { "workflow" } else { stem };
    format!("{stem}.yaml")
}

/// List saved workflow files (name + filename).
#[tauri::command]
pub fn list_workflows(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let dir = workflows_dir(&app)?;
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                let filename = path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or_default()
                    .to_string();
                // Try to read the spec name for a friendly label.
                let label = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| WorkflowSpec::parse(&s).ok())
                    .map(|sp| sp.name)
                    .unwrap_or_else(|| filename.clone());
                items.push(serde_json::json!({ "filename": filename, "name": label }));
            }
        }
    }
    items.sort_by(|a, b| {
        a["filename"].as_str().unwrap_or("").cmp(b["filename"].as_str().unwrap_or(""))
    });
    Ok(serde_json::json!(items))
}

/// Save a workflow source to `workflows/<name>.yaml`. Validates before writing
/// so the library never holds a broken DAG. Returns the filename used.
#[tauri::command]
pub fn save_workflow(
    app: tauri::AppHandle,
    name: String,
    source: String,
) -> Result<String, String> {
    // Validate structure before persisting.
    let spec = WorkflowSpec::parse(&source).map_err(|e| format!("解析失败：{e}"))?;
    Engine::plan(&spec).map_err(|e| format!("DAG 非法：{e}"))?;

    let dir = workflows_dir(&app)?;
    // Prefer an explicit name; fall back to the spec's own name.
    let base = if name.trim().is_empty() { spec.name.clone() } else { name };
    let filename = safe_filename(&base);
    std::fs::write(dir.join(&filename), source).map_err(|e| format!("写入失败：{e}"))?;
    Ok(filename)
}

/// Load a saved workflow's source by filename.
#[tauri::command]
pub fn load_workflow(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let dir = workflows_dir(&app)?;
    // Guard against path traversal: only accept a bare filename.
    let safe = std::path::Path::new(&filename)
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or("非法文件名")?;
    std::fs::read_to_string(dir.join(safe)).map_err(|e| format!("读取失败：{e}"))
}

/// Delete a saved workflow by filename.
#[tauri::command]
pub fn delete_workflow(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let dir = workflows_dir(&app)?;
    let safe = std::path::Path::new(&filename)
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or("非法文件名")?;
    std::fs::remove_file(dir.join(safe)).map_err(|e| format!("删除失败：{e}"))
}


/// Run a workflow, streaming `workflow://event` events to the frontend.
/// On success, if the run produced a SKILL, opens a chat session for it.
#[tauri::command]
pub async fn run_workflow(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    source: String,
    vars: std::collections::BTreeMap<String, String>,
) -> Result<serde_json::Value, String> {
    let mut spec = WorkflowSpec::parse(&source).map_err(|e| e.to_string())?;
    // Merge user-supplied vars over the spec defaults.
    for (k, v) in vars {
        spec.vars.insert(k, v);
    }

    let (settings, model) = {
        let s = state.settings.lock().unwrap();
        (s.clone(), s.model.clone())
    };
    spec.max_concurrency = spec.max_concurrency.min(settings.max_concurrency.max(1));

    let client = build_client(&settings)?;
    let engine = Engine::new(client, model).with_max_tokens(settings.max_tokens);

    let app2 = app.clone();
    let emit = Arc::new(move |ev: RunEvent| {
        let _ = app2.emit("workflow://event", &ev);
    });

    let ctx = engine.run(&spec, emit).await.map_err(|e| e.to_string())?;

    // If a skill was produced, register a chat session immediately.
    let skill = distill::extract_skill(&ctx);
    let session_id = if let Some(md) = &skill {
        let title = spec
            .vars
            .get("person")
            .cloned()
            .unwrap_or_else(|| spec.name.clone());
        let id = uuid::Uuid::new_v4().to_string();
        let session = ChatSession::new(id.clone(), title, md.clone());
        state.sessions.lock().unwrap().push(session);
        Some(id)
    } else {
        None
    };

    Ok(serde_json::json!({
        "outputs": ctx.outputs(),
        "skill": skill,
        "session_id": session_id,
    }))
}

// ---------- chat ----------

#[tauri::command]
pub fn list_sessions(state: State<AppState>) -> serde_json::Value {
    let sessions = state.sessions.lock().unwrap();
    let items: Vec<_> = sessions
        .iter()
        .map(|s| serde_json::json!({"id": s.id, "title": s.title}))
        .collect();
    serde_json::json!(items)
}

/// Create a chat session directly from a SKILL.md string (e.g. loading a saved
/// skill rather than running a full distillation).
#[tauri::command]
pub fn create_session(
    state: State<AppState>,
    title: String,
    skill_markdown: String,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let session = ChatSession::new(id.clone(), title, skill_markdown);
    state.sessions.lock().unwrap().push(session);
    id
}

/// Send a chat message into a session and stream the reply via
/// `chat://event`. Returns the full assistant text.
#[tauri::command]
pub async fn chat_send(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    message: String,
) -> Result<String, String> {
    // Snapshot the request payload while holding the lock briefly.
    let (settings, model, mut msgs) = {
        let s = state.settings.lock().unwrap();
        let settings = s.clone();
        let model = s.model.clone();
        let sessions = state.sessions.lock().unwrap();
        let session = sessions
            .iter()
            .find(|x| x.id == session_id)
            .ok_or("会话不存在")?;
        let mut msgs = session.messages();
        msgs.push(ChatMessage::user(message.clone()));
        (settings, model, msgs)
    };

    // Append the new user turn to history now.
    if let Some(s) = state
        .sessions
        .lock()
        .unwrap()
        .iter_mut()
        .find(|x| x.id == session_id)
    {
        s.push_user(message);
    }

    let client = build_client(&settings)?;
    let mut req = ChatRequest::new(model, std::mem::take(&mut msgs));
    req.stream = true;

    let app2 = app.clone();
    let sid = session_id.clone();
    let mut sink = move |delta: String| {
        let _ = app2.emit(
            "chat://event",
            &serde_json::json!({"session_id": sid, "delta": delta}),
        );
    };

    let reply = client
        .stream(&req, &mut sink)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(s) = state
        .sessions
        .lock()
        .unwrap()
        .iter_mut()
        .find(|x| x.id == session_id)
    {
        s.push_assistant(reply.clone());
    }
    Ok(reply)
}

// ---------- persistence helpers ----------

fn persist(app: &tauri::AppHandle, state: &State<AppState>) -> anyhow::Result<()> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(STORE_FILE)?;
    let s = state.settings.lock().unwrap();
    store.set("settings", serde_json::to_value(&*s)?);
    store.save()?;
    Ok(())
}

/// Load persisted settings into state at startup.
pub fn load_settings(app: &tauri::AppHandle) {
    use tauri_plugin_store::StoreExt;
    if let Ok(store) = app.store(STORE_FILE) {
        if let Some(v) = store.get("settings") {
            if let Ok(s) = serde_json::from_value::<ArkSettings>(v) {
                let state = app.state::<AppState>();
                *state.settings.lock().unwrap() = s;
            }
        }
    }
}
