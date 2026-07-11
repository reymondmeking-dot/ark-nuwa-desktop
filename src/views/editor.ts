import { api } from "../api";
import { errorMessage, escapeHtml, setButtonBusy } from "../dom";

let cached: string | null = null;
let dirty = false;

export async function renderEditor(el: HTMLElement) {
  if (cached === null) {
    cached = await api.defaultWorkflowYaml().catch(() => "name: demo\nnodes: []\n");
  }

  el.innerHTML = `
    <h1>Workflow 编辑器</h1>
    <p class="sub">用 YAML/JSON 定义 DAG。支持 <code>llm</code>、<code>synthesize</code>、<code>validate</code>、<code>generate_skill</code>、<code>test</code> 与 <code>passthrough</code> 节点。</p>

    <div class="card">
      <div class="section-heading">
        <label id="library-label">已保存的 Workflow（workflows/ 目录）</label>
        <button class="ghost compact" id="refresh" type="button">刷新列表</button>
      </div>
      <div id="library" class="workflow-list" aria-labelledby="library-label" aria-busy="true"><p class="hint">加载中…</p></div>
    </div>

    <div class="card">
      <label for="src">Workflow 源码</label>
      <textarea id="src" rows="20" spellcheck="false" aria-describedby="editor-help">${escapeHtml(cached)}</textarea>
      <p class="hint" id="editor-help">修改后先校验 DAG，再保存或用于运行。未保存内容会在当前应用会话内保留。</p>
      <div class="row save-row">
        <div>
          <label for="saveName">保存为（文件名，留空则使用 workflow 内的 name）</label>
          <input id="saveName" placeholder="my-workflow" autocomplete="off" spellcheck="false" />
        </div>
      </div>
      <div class="btn-row">
        <button class="primary" id="validate" type="button">校验 DAG</button>
        <button class="primary" id="save" type="button">保存到 workflows/</button>
        <button class="ghost" id="reset" type="button">恢复内置女娲蒸馏</button>
        <button class="ghost" id="touse" type="button">用于运行 ▶</button>
      </div>
      <div class="toast" id="toast" role="status" aria-live="polite"></div>
      <div id="dag" aria-live="polite"></div>
    </div>
  `;

  const sourceInput = el.querySelector("#src") as HTMLTextAreaElement;
  const toast = el.querySelector("#toast") as HTMLElement;
  const dag = el.querySelector("#dag") as HTMLElement;
  const library = el.querySelector("#library") as HTMLElement;
  const saveName = el.querySelector("#saveName") as HTMLInputElement;
  const validateButton = el.querySelector("#validate") as HTMLButtonElement;
  const saveButton = el.querySelector("#save") as HTMLButtonElement;
  const resetButton = el.querySelector("#reset") as HTMLButtonElement;
  const runButton = el.querySelector("#touse") as HTMLButtonElement;
  const refreshButton = el.querySelector("#refresh") as HTMLButtonElement;

  sourceInput.addEventListener("input", () => {
    cached = sourceInput.value;
    dirty = true;
  });

  const show = (message: string, ok: boolean) => {
    toast.textContent = message;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
    toast.setAttribute("role", ok ? "status" : "alert");
  };

  const setLibraryMessage = (message: string) => {
    library.replaceChildren();
    const paragraph = document.createElement("p");
    paragraph.className = "hint";
    paragraph.textContent = message;
    library.append(paragraph);
  };

  async function refreshLibrary() {
    library.setAttribute("aria-busy", "true");
    refreshButton.disabled = true;
    try {
      const items = await api.listWorkflows();
      if (!items.length) {
        setLibraryMessage("还没有保存的 workflow。编辑后点击“保存到 workflows/”。");
        return;
      }
      library.innerHTML = items
        .map(
          (workflow) => `
            <div class="workflow-item">
              <button class="pill wf-load" type="button" data-file="${escapeHtml(workflow.filename)}" title="加载 ${escapeHtml(workflow.filename)}">
                <span aria-hidden="true">📄</span> ${escapeHtml(workflow.name)}
              </button>
              <button class="wf-delete" type="button" data-file="${escapeHtml(workflow.filename)}" aria-label="删除 ${escapeHtml(workflow.name)}" title="删除">✕</button>
            </div>`,
        )
        .join("");

      library.querySelectorAll<HTMLButtonElement>(".wf-load").forEach((button) => {
        button.addEventListener("click", async () => {
          if (dirty && !window.confirm("当前编辑内容尚未保存，确定加载另一个 workflow 吗？")) return;
          setButtonBusy(button, true, "加载中…");
          try {
            const filename = button.dataset.file ?? "";
            cached = await api.loadWorkflow(filename);
            sourceInput.value = cached;
            dirty = false;
            dag.replaceChildren();
            show(`已加载 ${filename}`, true);
          } catch (error) {
            show(`加载失败：${errorMessage(error)}`, false);
          } finally {
            setButtonBusy(button, false, "加载中…");
          }
        });
      });

      library.querySelectorAll<HTMLButtonElement>(".wf-delete").forEach((button) => {
        button.addEventListener("click", async () => {
          const filename = button.dataset.file ?? "";
          if (!window.confirm(`确定删除 ${filename} 吗？此操作无法撤销。`)) return;
          setButtonBusy(button, true, "…");
          try {
            await api.deleteWorkflow(filename);
            show(`已删除 ${filename}`, true);
            await refreshLibrary();
          } catch (error) {
            show(`删除失败：${errorMessage(error)}`, false);
            setButtonBusy(button, false, "…");
          }
        });
      });
    } catch (error) {
      setLibraryMessage(`无法读取 workflow 目录：${errorMessage(error)}`);
    } finally {
      library.setAttribute("aria-busy", "false");
      refreshButton.disabled = false;
    }
  }

  validateButton.addEventListener("click", async () => {
    dag.replaceChildren();
    setButtonBusy(validateButton, true, "校验中…");
    try {
      const result = await api.validateWorkflow(sourceInput.value);
      show(`合法：${result.name}，共 ${result.node_count} 个节点、${result.layers.length} 层。`, true);
      dag.innerHTML = renderLayers(result.layers);
    } catch (error) {
      show(`校验失败：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(validateButton, false, "校验中…");
    }
  });

  saveButton.addEventListener("click", async () => {
    setButtonBusy(saveButton, true, "保存中…");
    try {
      await api.validateWorkflow(sourceInput.value);
      const filename = await api.saveWorkflow(saveName.value.trim(), sourceInput.value);
      cached = sourceInput.value;
      dirty = false;
      show(`已校验并保存为 ${filename}`, true);
      saveName.value = "";
      await refreshLibrary();
    } catch (error) {
      show(`保存失败：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(saveButton, false, "保存中…");
    }
  });

  resetButton.addEventListener("click", async () => {
    if (dirty && !window.confirm("确定放弃当前修改并恢复内置 workflow 吗？")) return;
    setButtonBusy(resetButton, true, "恢复中…");
    try {
      cached = await api.defaultWorkflowYaml();
      sourceInput.value = cached;
      dirty = false;
      dag.replaceChildren();
      show("已恢复内置女娲蒸馏闭环。", true);
    } catch (error) {
      show(`恢复失败：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(resetButton, false, "恢复中…");
    }
  });

  runButton.addEventListener("click", async () => {
    setButtonBusy(runButton, true, "校验中…");
    try {
      await api.validateWorkflow(sourceInput.value);
      sessionStorage.setItem("workflow_src", sourceInput.value);
      const runnerNav = document.querySelector<HTMLButtonElement>('.nav-btn[data-view="runner"]');
      runnerNav?.click();
    } catch (error) {
      show(`无法运行：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(runButton, false, "校验中…");
    }
  });

  refreshButton.addEventListener("click", () => void refreshLibrary());
  await refreshLibrary();
}

function renderLayers(layers: string[][]): string {
  const html = layers
    .map(
      (layer, index) =>
        `<div class="layer"><span class="layer-label">第 ${index + 1} 层（并行执行）</span>${layer
          .map((id) => `<div class="node"><div class="nid">${escapeHtml(id)}</div></div>`)
          .join("")}</div>`,
    )
    .join("");
  return `<div class="layers dag-result">${html}</div>`;
}
