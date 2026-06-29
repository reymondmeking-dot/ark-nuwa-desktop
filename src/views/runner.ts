import { api, type RunEvent, type NodeStatus } from "../api";

// ---- Module-level run state (survives view switches) -------------------------
// The runner view can be unmounted/remounted while a workflow is still running.
// We keep all progress here and restore the DOM from it on every render, and we
// register the Tauri event listener exactly once so no events are missed.

interface RunState {
  src: string;
  running: boolean;
  finishedOk: boolean | null;
  sessionId: string | null;
  statuses: Record<string, NodeStatus>;
  stream: string;
  toast: { msg: string; ok: boolean } | null;
}

let state: RunState = blankState("");
let listenerReady = false;
// Currently mounted DOM (or null when the runner view isn't visible).
let mounted: { root: HTMLElement; stream: HTMLElement } | null = null;

function blankState(src: string): RunState {
  return {
    src,
    running: false,
    finishedOk: null,
    sessionId: null,
    statuses: {},
    stream: "",
    toast: null,
  };
}

export async function renderRunner(el: HTMLElement) {
  // Register the global event listener once; it updates state (always) and the
  // DOM (when mounted), independent of view switches.
  if (!listenerReady) {
    listenerReady = true;
    await api.onWorkflowEvent((ev) => applyEvent(ev));
  }

  const src =
    sessionStorage.getItem("workflow_src") ||
    (await api.defaultWorkflowYaml().catch(() => ""));

  // If the workflow source changed since the last run, reset progress — unless a
  // run is currently in flight for the old source (don't clobber live progress).
  if (state.src !== src && !state.running) {
    state = blankState(src);
  }

  // Plan to draw the DAG and discover variables.
  let layers: string[][] = [];
  let planName = "";
  try {
    const r = await api.validateWorkflow(src);
    layers = r.layers;
    planName = r.name;
  } catch {
    /* shown below */
  }

  const vars = extractVars(src);

  el.innerHTML = `
    <h1>运行 · ${planName || "Workflow"}</h1>
    <p class="sub">填入变量后运行。研究节点并行执行；验证不通过会回退重试（闭环）。${
      state.running ? ' <strong style="color:var(--accent)">● 运行中</strong>' : ""
    }</p>
    <div class="card">
      <div id="vars">${
        vars.length
          ? vars
              .map(
                (v) =>
                  `<label>${v}</label><input class="var" data-key="${v}" value="${defaultVar(src, v)}" />`
              )
              .join("")
          : '<p class="hint">该 workflow 未声明变量。</p>'
      }</div>
      <div class="btn-row">
        <button class="primary" id="run" ${state.running ? "disabled" : ""}>▶ 运行 workflow</button>
        <button class="ghost" id="tochat" ${state.sessionId ? "" : "disabled"}>蒸馏完成 → 去对话 💬</button>
      </div>
      <div class="toast" id="toast"></div>
    </div>

    <div class="card">
      <div id="dag">${renderLayers(layers)}</div>
      <label>实时输出</label>
      <div class="stream-box" id="stream"></div>
    </div>
  `;

  const toast = el.querySelector("#toast") as HTMLElement;
  const stream = el.querySelector("#stream") as HTMLElement;
  const tochat = el.querySelector("#tochat") as HTMLButtonElement;
  mounted = { root: el, stream };

  const showToast = (m: string, ok: boolean) => {
    state.toast = { msg: m, ok };
    toast.textContent = m;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
  };

  // Restore visuals from saved state (so switching back mid-run keeps progress).
  restoreDom(el, stream);
  if (state.toast) {
    toast.textContent = state.toast.msg;
    toast.className = `toast show ${state.toast.ok ? "ok" : "err"}`;
  }

  el.querySelector("#run")!.addEventListener("click", async () => {
    if (state.running) return;
    const collected: Record<string, string> = {};
    el.querySelectorAll(".var").forEach((i) => {
      const inp = i as HTMLInputElement;
      collected[inp.dataset.key!] = inp.value;
    });

    // Fresh run: reset state and DOM.
    state = blankState(src);
    state.running = true;
    restoreDom(el, stream);
    showToast("⏳ 运行中…", true);
    (el.querySelector("#run") as HTMLButtonElement).disabled = true;
    tochat.disabled = true;
    refreshRunningBadge(el);

    try {
      const r = await api.runWorkflow(src, collected);
      state.sessionId = r.session_id;
      if (r.session_id) {
        sessionStorage.setItem("active_session", r.session_id);
        showToast("✅ 蒸馏完成，已生成对话智能体", true);
      } else {
        showToast("✅ Workflow 完成（未产出 skill）", true);
      }
    } catch (e) {
      showToast("❌ 运行失败：" + e, false);
    } finally {
      state.running = false;
      // Re-enable controls if still mounted.
      const runBtn = el.querySelector("#run") as HTMLButtonElement | null;
      if (runBtn) runBtn.disabled = false;
      const tc = el.querySelector("#tochat") as HTMLButtonElement | null;
      if (tc) tc.disabled = !state.sessionId;
      refreshRunningBadge(el);
    }
  });

  tochat.addEventListener("click", () =>
    (document.querySelector('.nav-btn[data-view="chat"]') as HTMLElement).click()
  );
}

// Apply an incoming event to state, and to the DOM if currently mounted.
function applyEvent(ev: RunEvent) {
  switch (ev.kind) {
    case "node_status":
      state.statuses[ev.node] = ev.status;
      if (mounted) setNodeStatus(mounted.root, ev.node, ev.status);
      break;
    case "node_chunk":
      state.stream += ev.delta;
      if (mounted) appendStream(mounted.stream, ev.delta);
      break;
    case "node_output":
      state.stream += `\n\n— [${ev.node}] 完成 —\n`;
      if (mounted) appendStream(mounted.stream, `\n\n— [${ev.node}] 完成 —\n`);
      break;
    case "loop_back": {
      const line = `\n\n🔁 验证未通过：回退到「${ev.to}」重试（第 ${ev.attempt} 次）\n`;
      state.stream += line;
      if (mounted) appendStream(mounted.stream, line);
      break;
    }
    case "finished": {
      state.finishedOk = ev.ok;
      const line = ev.ok ? "\n\n🏁 全部完成。\n" : "\n\n🛑 运行终止。\n";
      state.stream += line;
      if (mounted) appendStream(mounted.stream, line);
      break;
    }
  }
}

function appendStream(stream: HTMLElement, text: string) {
  stream.textContent += text;
  stream.scrollTop = stream.scrollHeight;
}

// Rebuild DOM visuals from saved state after a (re)render.
function restoreDom(root: HTMLElement, stream: HTMLElement) {
  root.querySelectorAll<HTMLElement>(".node").forEach((n) => {
    const id = n.dataset.id!;
    const st = state.statuses[id] ?? "pending";
    n.className = `node ${st}`;
    const b = n.querySelector(".badge") as HTMLElement;
    if (b) {
      b.className = st === "pending" ? "badge" : `badge ${st}`;
      b.textContent = labelFor(st);
    }
  });
  stream.textContent = state.stream;
  stream.scrollTop = stream.scrollHeight;
}

function refreshRunningBadge(root: HTMLElement) {
  const sub = root.querySelector(".sub");
  if (!sub) return;
  const existing = sub.querySelector("strong");
  if (state.running && !existing) {
    const s = document.createElement("strong");
    s.style.color = "var(--accent)";
    s.textContent = " ● 运行中";
    sub.appendChild(s);
  } else if (!state.running && existing) {
    existing.remove();
  }
}

function nodeEl(root: HTMLElement, id: string): HTMLElement | null {
  return root.querySelector(`.node[data-id="${id}"]`);
}
function setNodeStatus(root: HTMLElement, id: string, status: NodeStatus) {
  const n = nodeEl(root, id);
  if (!n) return;
  n.className = `node ${status}`;
  const badge = n.querySelector(".badge") as HTMLElement;
  if (badge) {
    badge.className = `badge ${status}`;
    badge.textContent = labelFor(status);
  }
}
function labelFor(s: NodeStatus): string {
  return { pending: "", running: "运行", done: "完成", failed: "失败", retrying: "重试", skipped: "跳过" }[s];
}

function renderLayers(layers: string[][]): string {
  if (!layers.length) return '<p class="hint">DAG 校验失败，请回编辑器修正。</p>';
  const html = layers
    .map(
      (layer, i) =>
        `<div class="layer"><span class="layer-label">第 ${i + 1} 层</span>${layer
          .map(
            (id) =>
              `<div class="node pending" data-id="${id}"><div class="nid">${id}</div><span class="badge"></span></div>`
          )
          .join("")}</div>`
    )
    .join("");
  return `<div class="layers">${html}</div>`;
}

// Naive var discovery: collect {{key}} tokens that are not produced as outputs.
function extractVars(src: string): string[] {
  const toks = new Set<string>();
  const re = /\{\{\s*([a-zA-Z0-9_]+)\s*\}\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src))) toks.add(m[1]);
  const outputs = new Set<string>();
  const ro = /output:\s*([a-zA-Z0-9_]+)/g;
  while ((m = ro.exec(src))) outputs.add(m[1]);
  const ids = new Set<string>();
  const ri = /id:\s*([a-zA-Z0-9_]+)/g;
  while ((m = ri.exec(src))) ids.add(m[1]);
  return [...toks].filter((t) => !outputs.has(t) && !ids.has(t));
}

function defaultVar(src: string, key: string): string {
  // Pull a default from a `vars:` block if present (best-effort).
  const re = new RegExp(`${key}\\s*:\\s*["']?([^"'\\n]+)["']?`);
  const m = re.exec(src.split("nodes:")[0] || "");
  return m ? m[1].trim() : "";
}
