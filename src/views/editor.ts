import { api } from "../api";

// Persist editor content across view switches (in-memory).
let cached: string | null = null;

export async function renderEditor(el: HTMLElement) {
  if (cached === null) {
    cached = await api.defaultWorkflowYaml().catch(() => "name: demo\nnodes: []\n");
  }

  el.innerHTML = `
    <h1>Workflow 编辑器</h1>
    <p class="sub">YAML/JSON 定义 DAG。节点类型：<code>llm</code> <code>synthesize</code> <code>validate</code> <code>generate_skill</code> <code>test</code> <code>passthrough</code>。<code>validate</code> 可配 <code>on_fail.goto</code> 形成闭环。</p>

    <div class="card">
      <label>已保存的 Workflow（workflows/ 目录）</label>
      <div id="library"><p class="hint">加载中…</p></div>
    </div>

    <div class="card">
      <textarea id="src" rows="20">${escapeHtml(cached)}</textarea>
      <div class="row" style="margin-top:12px">
        <div style="flex:2">
          <label>保存为（文件名，留空则用 workflow 内的 name）</label>
          <input id="saveName" placeholder="my-workflow" />
        </div>
      </div>
      <div class="btn-row">
        <button class="primary" id="validate">校验 DAG</button>
        <button class="primary" id="save">💾 保存到 workflows/</button>
        <button class="ghost" id="reset">恢复内置女娲蒸馏</button>
        <button class="ghost" id="touse">用于运行 ▶</button>
      </div>
      <div class="toast" id="toast"></div>
      <div id="dag"></div>
    </div>
  `;

  const src = el.querySelector("#src") as HTMLTextAreaElement;
  const toast = el.querySelector("#toast") as HTMLElement;
  const dag = el.querySelector("#dag") as HTMLElement;
  const library = el.querySelector("#library") as HTMLElement;
  const saveName = el.querySelector("#saveName") as HTMLInputElement;
  src.addEventListener("input", () => (cached = src.value));

  const show = (m: string, ok: boolean) => {
    toast.textContent = m;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
  };

  async function refreshLibrary() {
    try {
      const items = await api.listWorkflows();
      if (!items.length) {
        library.innerHTML = '<p class="hint">还没有保存的 workflow。编辑后点「保存到 workflows/」。</p>';
        return;
      }
      library.innerHTML = items
        .map(
          (w) =>
            `<span class="pill" data-file="${w.filename}" title="${w.filename}">📄 ${escapeHtml(w.name)}
               <span class="wf-del" data-file="${w.filename}" style="margin-left:8px;opacity:.6">✕</span>
             </span>`
        )
        .join("");
      // Load on click.
      library.querySelectorAll<HTMLElement>(".pill").forEach((p) =>
        p.addEventListener("click", async (ev) => {
          if ((ev.target as HTMLElement).classList.contains("wf-del")) return;
          const file = p.dataset.file!;
          try {
            cached = await api.loadWorkflow(file);
            src.value = cached;
            show(`已加载 ${file}`, true);
          } catch (e) {
            show("加载失败：" + e, false);
          }
        })
      );
      // Delete on ✕.
      library.querySelectorAll<HTMLElement>(".wf-del").forEach((x) =>
        x.addEventListener("click", async (ev) => {
          ev.stopPropagation();
          const file = x.dataset.file!;
          try {
            await api.deleteWorkflow(file);
            show(`已删除 ${file}`, true);
            refreshLibrary();
          } catch (e) {
            show("删除失败：" + e, false);
          }
        })
      );
    } catch (e) {
      library.innerHTML = `<p class="hint">无法读取目录：${e}</p>`;
    }
  }

  el.querySelector("#validate")!.addEventListener("click", async () => {
    dag.innerHTML = "";
    try {
      const r = await api.validateWorkflow(src.value);
      show(`✅ 合法：${r.name}，共 ${r.node_count} 个节点，${r.layers.length} 层`, true);
      dag.innerHTML = renderLayers(r.layers);
    } catch (e) {
      show("❌ " + e, false);
    }
  });

  el.querySelector("#save")!.addEventListener("click", async () => {
    try {
      const file = await api.saveWorkflow(saveName.value.trim(), src.value);
      show(`✅ 已保存为 ${file}`, true);
      saveName.value = "";
      refreshLibrary();
    } catch (e) {
      show("❌ 保存失败：" + e, false);
    }
  });

  el.querySelector("#reset")!.addEventListener("click", async () => {
    cached = await api.defaultWorkflowYaml();
    src.value = cached;
    show("已恢复内置女娲蒸馏闭环", true);
  });

  el.querySelector("#touse")!.addEventListener("click", () => {
    sessionStorage.setItem("workflow_src", src.value);
    (document.querySelector('.nav-btn[data-view="runner"]') as HTMLElement).click();
  });

  refreshLibrary();
}

function renderLayers(layers: string[][]): string {
  const html = layers
    .map(
      (layer, i) =>
        `<div class="layer"><span class="layer-label">第 ${i + 1} 层（并行执行）</span>${layer
          .map((id) => `<div class="node"><div>${id}</div></div>`)
          .join("")}</div>`
    )
    .join("");
  return `<div class="layers" style="margin-top:16px">${html}</div>`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
