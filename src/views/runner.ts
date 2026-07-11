import { api, type NodeStatus, type RunEvent } from "../api";
import { errorMessage, escapeHtml, setButtonBusy } from "../dom";

interface RunState {
  src: string;
  running: boolean;
  finishedOk: boolean | null;
  sessionId: string | null;
  statuses: Record<string, NodeStatus>;
  inputs: Record<string, string>;
  stream: string;
  toast: { msg: string; ok: boolean } | null;
}

const MAX_STREAM_LENGTH = 250_000;
let state: RunState = blankState("");
let listenerPromise: Promise<void> | null = null;
let mounted: { root: HTMLElement; stream: HTMLElement } | null = null;
let streamFrame: number | null = null;

function blankState(src: string): RunState {
  return {
    src,
    running: false,
    finishedOk: null,
    sessionId: null,
    statuses: {},
    inputs: {},
    stream: "",
    toast: null,
  };
}

function ensureWorkflowListener(): Promise<void> {
  if (!listenerPromise) {
    listenerPromise = api
      .onWorkflowEvent(applyEvent)
      .then(() => undefined)
      .catch((error) => {
        listenerPromise = null;
        throw error;
      });
  }
  return listenerPromise;
}

export async function renderRunner(el: HTMLElement) {
  const storedSource = sessionStorage.getItem("workflow_src");
  const [listenerError, requestedSource] = await Promise.all([
    ensureWorkflowListener().then(() => null).catch((error: unknown) => errorMessage(error)),
    storedSource
      ? Promise.resolve(storedSource)
      : api.defaultWorkflowYaml().catch(() => ""),
  ]);

  if (state.src !== requestedSource && !state.running) state = blankState(requestedSource);
  const source = state.running ? state.src : requestedSource;

  let layers: string[][] = [];
  let planName = "";
  let validationError = "";
  try {
    const result = await api.validateWorkflow(source);
    layers = result.layers;
    planName = result.name;
  } catch (error) {
    validationError = errorMessage(error);
  }

  const variables = extractVars(source);
  const canRun = source.length > 0 && validationError.length === 0 && layers.length > 0;
  el.innerHTML = `
    <h1>运行 · ${escapeHtml(planName || "Workflow")}</h1>
    <p class="sub">填入变量后运行。研究节点会并行执行；验证不通过时按 workflow 配置回退重试。<span class="running-label" ${state.running ? "" : "hidden"}>● 运行中</span></p>
    <div class="card">
      <div id="vars">${
        variables.length
          ? variables
              .map((variable, index) => {
                const value = state.inputs[variable] ?? defaultVar(source, variable);
                return `<label for="var-${index}">${escapeHtml(variable)}</label><input id="var-${index}" class="var" data-key="${escapeHtml(variable)}" value="${escapeHtml(value)}" autocomplete="off" />`;
              })
              .join("")
          : '<p class="hint">该 workflow 未声明外部变量。</p>'
      }</div>
      <div class="btn-row">
        <button class="primary" id="run" type="button" ${state.running || !canRun ? "disabled" : ""}>运行 workflow</button>
        <button class="ghost" id="tochat" type="button" ${state.sessionId ? "" : "disabled"}>蒸馏完成 → 去对话</button>
      </div>
      <div class="toast" id="toast" role="status" aria-live="polite"></div>
    </div>

    <div class="card">
      <div id="dag">${renderLayers(layers, validationError)}</div>
      <div class="section-heading stream-heading">
        <label id="stream-label">实时输出</label>
        <button class="ghost compact" id="clear-stream" type="button" ${state.stream ? "" : "disabled"}>清空输出</button>
      </div>
      <div class="stream-box" id="stream" role="log" aria-labelledby="stream-label" aria-live="polite" aria-relevant="additions text" tabindex="0"></div>
    </div>
  `;

  const toast = el.querySelector("#toast") as HTMLElement;
  const stream = el.querySelector("#stream") as HTMLElement;
  const runButton = el.querySelector("#run") as HTMLButtonElement;
  const chatButton = el.querySelector("#tochat") as HTMLButtonElement;
  const clearButton = el.querySelector("#clear-stream") as HTMLButtonElement;
  mounted = { root: el, stream };

  const showToast = (message: string, ok: boolean) => {
    state.toast = { msg: message, ok };
    toast.textContent = message;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
    toast.setAttribute("role", ok ? "status" : "alert");
  };

  restoreDom(el, stream);
  if (state.toast) showToast(state.toast.msg, state.toast.ok);
  else if (listenerError) {
    toast.textContent = `实时事件监听暂不可用，运行结果仍会正常返回：${listenerError}`;
    toast.className = "toast show err";
    toast.setAttribute("role", "alert");
  } else if (validationError) {
    toast.textContent = `Workflow 校验失败：${validationError}`;
    toast.className = "toast show err";
    toast.setAttribute("role", "alert");
  }

  el.querySelectorAll<HTMLInputElement>(".var").forEach((input) => {
    input.addEventListener("input", () => {
      const key = input.dataset.key;
      if (key) state.inputs[key] = input.value;
    });
  });

  runButton.addEventListener("click", async () => {
    if (state.running || !canRun) return;
    const collected: Record<string, string> = {};
    el.querySelectorAll<HTMLInputElement>(".var").forEach((input) => {
      const key = input.dataset.key;
      if (key) collected[key] = input.value;
    });

    state = { ...blankState(source), running: true, inputs: collected };
    restoreDom(el, stream);
    showToast("Workflow 正在运行…", true);
    setButtonBusy(runButton, true, "运行中…");
    chatButton.disabled = true;
    clearButton.disabled = true;
    refreshRunningBadge(el);

    try {
      const result = await api.runWorkflow(source, collected);
      state.sessionId = result.session_id;
      if (result.session_id) {
        sessionStorage.setItem("active_session", result.session_id);
        showToast("蒸馏完成，已生成可对话智能体。", true);
      } else {
        showToast("Workflow 已完成，本次未产出 Skill。", true);
      }
    } catch (error) {
      showToast(`运行失败：${errorMessage(error)}`, false);
    } finally {
      state.running = false;
      setButtonBusy(runButton, false, "运行中…");
      chatButton.disabled = !state.sessionId;
      clearButton.disabled = state.stream.length === 0;
      refreshRunningBadge(el);
    }
  });

  chatButton.addEventListener("click", () => {
    document.querySelector<HTMLButtonElement>('.nav-btn[data-view="chat"]')?.click();
  });

  clearButton.addEventListener("click", () => {
    state.stream = "";
    stream.textContent = "";
    clearButton.disabled = true;
    stream.focus();
  });
}

function applyEvent(event: RunEvent) {
  switch (event.kind) {
    case "node_status":
      state.statuses[event.node] = event.status;
      if (mounted) setNodeStatus(mounted.root, event.node, event.status);
      break;
    case "node_chunk":
      addStreamText(event.delta);
      break;
    case "node_output":
      addStreamText(`\n\n— [${event.node}] 完成 —\n`);
      break;
    case "loop_back":
      addStreamText(`\n\n↻ 验证未通过：回退到“${event.to}”重试（第 ${event.attempt} 次）\n`);
      break;
    case "finished":
      state.finishedOk = event.ok;
      addStreamText(event.ok ? "\n\n全部完成。\n" : "\n\n运行终止。\n");
      break;
    case "log":
      addStreamText(`\n${event.message}\n`);
      break;
  }
}

function addStreamText(text: string) {
  state.stream += text;
  if (state.stream.length > MAX_STREAM_LENGTH) {
    state.stream = `…较早输出已截断…\n${state.stream.slice(-MAX_STREAM_LENGTH)}`;
  }
  scheduleStreamRender();
}

function scheduleStreamRender() {
  if (!mounted || streamFrame !== null) return;
  streamFrame = window.requestAnimationFrame(() => {
    streamFrame = null;
    if (!mounted) return;
    mounted.stream.textContent = state.stream;
    mounted.stream.scrollTop = mounted.stream.scrollHeight;
    const clearButton = mounted.root.querySelector<HTMLButtonElement>("#clear-stream");
    if (clearButton && !state.running) clearButton.disabled = state.stream.length === 0;
  });
}

function restoreDom(root: HTMLElement, stream: HTMLElement) {
  root.querySelectorAll<HTMLElement>(".node").forEach((node) => {
    const id = node.dataset.id ?? "";
    const status = state.statuses[id] ?? "pending";
    applyNodeStatus(node, status);
  });
  stream.textContent = state.stream;
  stream.scrollTop = stream.scrollHeight;
}

function refreshRunningBadge(root: HTMLElement) {
  const label = root.querySelector<HTMLElement>(".running-label");
  if (label) label.hidden = !state.running;
}

function setNodeStatus(root: HTMLElement, id: string, status: NodeStatus) {
  const node = Array.from(root.querySelectorAll<HTMLElement>(".node")).find(
    (candidate) => candidate.dataset.id === id,
  );
  if (node) applyNodeStatus(node, status);
}

function applyNodeStatus(node: HTMLElement, status: NodeStatus) {
  node.className = `node ${status}`;
  node.setAttribute("aria-label", `${node.dataset.id ?? "节点"}：${labelFor(status) || "等待"}`);
  const badge = node.querySelector<HTMLElement>(".badge");
  if (!badge) return;
  badge.className = status === "pending" ? "badge" : `badge ${status}`;
  badge.textContent = labelFor(status);
}

function labelFor(status: NodeStatus): string {
  return {
    pending: "",
    running: "运行",
    done: "完成",
    failed: "失败",
    retrying: "重试",
    skipped: "跳过",
  }[status];
}

function renderLayers(layers: string[][], validationError: string): string {
  if (!layers.length) {
    const message = validationError
      ? `DAG 校验失败：${validationError}`
      : "该 workflow 暂无可执行节点。";
    return `<p class="hint">${escapeHtml(message)}</p>`;
  }
  const html = layers
    .map(
      (layer, index) =>
        `<div class="layer"><span class="layer-label">第 ${index + 1} 层</span>${layer
          .map(
            (id) =>
              `<div class="node pending" data-id="${escapeHtml(id)}" aria-label="${escapeHtml(id)}：等待"><div class="nid">${escapeHtml(id)}</div><span class="badge"></span></div>`,
          )
          .join("")}</div>`,
    )
    .join("");
  return `<div class="layers">${html}</div>`;
}

function extractVars(source: string): string[] {
  const tokens = new Set<string>();
  const tokenPattern = /\{\{\s*([a-zA-Z0-9_]+)\s*\}\}/g;
  let match: RegExpExecArray | null;
  while ((match = tokenPattern.exec(source))) tokens.add(match[1]);

  const outputs = new Set<string>();
  const outputPattern = /output:\s*([a-zA-Z0-9_]+)/g;
  while ((match = outputPattern.exec(source))) outputs.add(match[1]);

  const ids = new Set<string>();
  const idPattern = /id:\s*([a-zA-Z0-9_]+)/g;
  while ((match = idPattern.exec(source))) ids.add(match[1]);
  return [...tokens].filter((token) => !outputs.has(token) && !ids.has(token));
}

function defaultVar(source: string, key: string): string {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`${escapedKey}\\s*:\\s*["']?([^"'\\n]+)["']?`);
  const match = pattern.exec(source.split("nodes:")[0] || "");
  return match ? match[1].trim() : "";
}
