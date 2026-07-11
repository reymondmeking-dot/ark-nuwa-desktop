import { api, type SettingsPayload, type SettingsView } from "../api";
import { errorMessage, escapeHtml, setButtonBusy } from "../dom";

const ANTHROPIC_URL = "https://ark.cn-beijing.volces.com/api/coding";
const OPENAI_URL = "https://ark.cn-beijing.volces.com/api/coding/v3";

const DEFAULT_SETTINGS: SettingsView = {
  base_url: ANTHROPIC_URL,
  model: "ark-code-latest",
  protocol: "anthropic",
  temperature: 0.7,
  max_tokens: 4096,
  max_concurrency: 6,
  timeout_secs: 120,
  has_api_key: false,
};

export async function renderSettings(el: HTMLElement) {
  const [settings, models] = await Promise.all([
    api.getSettings().catch(() => DEFAULT_SETTINGS),
    api.codingModels().catch(() => ["ark-code-latest"]),
  ]);
  const isAnthropic =
    (settings.protocol || "anthropic").toLowerCase() === "anthropic";

  el.innerHTML = `
    <h1>设置 · 火山方舟 Ark Coding Plan 接入</h1>
    <p class="sub">单独调用 Ark Coding Plan。默认 Anthropic 协议（与 Claude Code 相同，<code>ark-code-latest</code> 在此可用）。</p>
    <form class="card" id="settings-form">
      <label for="protocol">协议（与 Base URL 联动）</label>
      <select id="protocol" name="protocol">
        <option value="anthropic" ${isAnthropic ? "selected" : ""}>Anthropic 协议（/api/coding · 推荐，支持 ark-code-latest）</option>
        <option value="openai" ${!isAnthropic ? "selected" : ""}>OpenAI 协议（/api/coding/v3 · 需用具体模型名）</option>
      </select>
      <p class="hint">托管别名 <code>ark-code-latest</code> 只在 Anthropic 接口解析；OpenAI 接口需填写具体模型名。</p>

      <label for="base_url">Base URL</label>
      <input id="base_url" name="base_url" type="url" required readonly spellcheck="false" value="${escapeHtml(settings.base_url)}" placeholder="${ANTHROPIC_URL}" />
      <p class="hint">不要使用 <code>/api/v3</code>，它不走 Coding Plan 额度，可能产生额外费用。</p>

      <label for="model">模型（model）</label>
      <input id="model" name="model" list="model_list" required spellcheck="false" value="${escapeHtml(settings.model)}" placeholder="ark-code-latest 或具体模型名" />
      <datalist id="model_list">
        ${models.map((model) => `<option value="${escapeHtml(model)}"></option>`).join("")}
      </datalist>
      <p class="hint"><code>ark-code-latest</code> 需先在控制台「开通管理」选择生效模型，也可直接填写具体模型名。</p>

      <label for="api_key">API Key（Bearer Token）${settings.has_api_key ? " — 已保存，留空表示不修改" : ""}</label>
      <input id="api_key" name="api_key" type="password" autocomplete="off" spellcheck="false" placeholder="${settings.has_api_key ? "•••••••• 已保存" : "粘贴已订阅 Coding Plan 的 API Key"}" />

      <div class="row settings-grid">
        <div>
          <label for="temperature">temperature</label>
          <input id="temperature" name="temperature" type="number" step="0.1" min="0" max="2" value="${settings.temperature}" />
        </div>
        <div>
          <label for="max_tokens">max_tokens（单次回复上限）</label>
          <input id="max_tokens" name="max_tokens" type="number" min="1" max="65536" value="${settings.max_tokens}" />
        </div>
        <div>
          <label for="max_concurrency">最大并发</label>
          <input id="max_concurrency" name="max_concurrency" type="number" min="1" max="16" value="${settings.max_concurrency}" />
        </div>
        <div>
          <label for="timeout_secs">超时（秒）</label>
          <input id="timeout_secs" name="timeout_secs" type="number" min="5" max="3600" value="${settings.timeout_secs}" />
        </div>
      </div>

      <div class="btn-row">
        <button class="primary" id="save" type="submit">保存配置</button>
        <button class="ghost" id="test" type="button">保存并测试连接</button>
      </div>
      <div class="toast" id="toast" role="status" aria-live="polite"></div>
      <p class="hint">密钥保存在操作系统凭据库，不会写入 JSON 配置、界面或工作流文件。</p>
    </form>
  `;

  const form = el.querySelector("#settings-form") as HTMLFormElement;
  const toast = el.querySelector("#toast") as HTMLElement;
  const saveButton = el.querySelector("#save") as HTMLButtonElement;
  const testButton = el.querySelector("#test") as HTMLButtonElement;
  const baseUrlInput = el.querySelector("#base_url") as HTMLInputElement;
  const protocolSelect = el.querySelector("#protocol") as HTMLSelectElement;

  const show = (message: string, ok: boolean) => {
    toast.textContent = message;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
    toast.setAttribute("role", ok ? "status" : "alert");
  };

  protocolSelect.addEventListener("change", () => {
    const current = baseUrlInput.value.trim();
    if (current === ANTHROPIC_URL || current === OPENAI_URL || current === "") {
      baseUrlInput.value =
        protocolSelect.value === "anthropic" ? ANTHROPIC_URL : OPENAI_URL;
    }
  });

  const readPayload = (): SettingsPayload => {
    if (!form.reportValidity()) throw new Error("请先补全或修正表单中的配置。");

    const value = (id: string) =>
      (el.querySelector(`#${id}`) as HTMLInputElement).value;
    const baseUrl = value("base_url").trim();
    const parsedUrl = new URL(baseUrl);
    if (parsedUrl.protocol !== "https:") {
      throw new Error("Base URL 必须使用 HTTPS。");
    }
    if (
      parsedUrl.hostname !== "ark.cn-beijing.volces.com" ||
      parsedUrl.username ||
      parsedUrl.password ||
      (parsedUrl.port && parsedUrl.port !== "443")
    ) {
      throw new Error("Base URL 只能使用官方 Ark HTTPS 域名。");
    }
    if (parsedUrl.search || parsedUrl.hash) {
      throw new Error("Base URL 不能包含查询参数或片段。");
    }
    const expectedPath =
      protocolSelect.value === "anthropic" ? "/api/coding" : "/api/coding/v3";
    if (parsedUrl.pathname.replace(/\/+$/, "") !== expectedPath) {
      throw new Error(`当前协议的 Base URL 路径必须是 ${expectedPath}。`);
    }
    const model = value("model").trim();
    if (!model) throw new Error("模型名称不能为空。");

    const clampInteger = (id: string, fallback: number, min: number, max: number) => {
      const parsed = Number.parseInt(value(id), 10);
      return Number.isFinite(parsed)
        ? Math.min(max, Math.max(min, parsed))
        : fallback;
    };
    const parsedTemperature = Number.parseFloat(value("temperature"));

    return {
      base_url: baseUrl.replace(/\/+$/, ""),
      model,
      protocol: protocolSelect.value,
      api_key: value("api_key"),
      temperature: Number.isFinite(parsedTemperature)
        ? Math.min(2, Math.max(0, parsedTemperature))
        : DEFAULT_SETTINGS.temperature,
      max_tokens: clampInteger("max_tokens", 4096, 1, 65536),
      max_concurrency: clampInteger("max_concurrency", 6, 1, 16),
      timeout_secs: clampInteger("timeout_secs", 120, 5, 3600),
    };
  };

  const persistSettings = async () => {
    const payload = readPayload();
    const message = await api.saveSettings(payload);
    (el.querySelector("#temperature") as HTMLInputElement).value = String(payload.temperature);
    (el.querySelector("#max_tokens") as HTMLInputElement).value = String(payload.max_tokens);
    (el.querySelector("#max_concurrency") as HTMLInputElement).value = String(payload.max_concurrency);
    (el.querySelector("#timeout_secs") as HTMLInputElement).value = String(payload.timeout_secs);
    return message;
  };

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    setButtonBusy(saveButton, true, "保存中…");
    testButton.disabled = true;
    try {
      const message = await persistSettings();
      show(message, true);
    } catch (error) {
      show(`保存失败：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(saveButton, false, "保存中…");
      testButton.disabled = false;
    }
  });

  testButton.addEventListener("click", async () => {
    setButtonBusy(testButton, true, "测试中…");
    saveButton.disabled = true;
    show("正在保存当前配置并测试连接…", true);
    try {
      const saveMessage = await persistSettings();
      const result = await api.testConnection();
      show(`${saveMessage} ${result}`, true);
    } catch (error) {
      show(`连接失败：${errorMessage(error)}`, false);
    } finally {
      setButtonBusy(testButton, false, "测试中…");
      saveButton.disabled = false;
    }
  });
}
