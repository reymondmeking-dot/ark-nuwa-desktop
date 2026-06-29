import { api, type SettingsView } from "../api";

const ANTHROPIC_URL = "https://ark.cn-beijing.volces.com/api/coding";
const OPENAI_URL = "https://ark.cn-beijing.volces.com/api/coding/v3";

export async function renderSettings(el: HTMLElement) {
  const s: SettingsView = await api.getSettings().catch(() => ({
    base_url: ANTHROPIC_URL,
    model: "ark-code-latest",
    protocol: "anthropic",
    temperature: 0.7,
    max_tokens: 4096,
    max_concurrency: 6,
    timeout_secs: 120,
    has_api_key: false,
  }));
  const models = await api.codingModels().catch(() => ["ark-code-latest"]);
  const isAnthropic = (s.protocol || "anthropic").toLowerCase() === "anthropic";

  el.innerHTML = `
    <h1>设置 · 火山方舟 Ark Coding Plan 接入</h1>
    <p class="sub">单独调用 Ark Coding Plan。默认 Anthropic 协议（与 Claude Code 相同，<code>ark-code-latest</code> 在此可用）。</p>
    <div class="card">
      <label>协议（与 base_url 联动）</label>
      <select id="protocol">
        <option value="anthropic" ${isAnthropic ? "selected" : ""}>Anthropic 协议（/api/coding · 推荐，支持 ark-code-latest）</option>
        <option value="openai" ${!isAnthropic ? "selected" : ""}>OpenAI 协议（/api/coding/v3 · 需用具体模型名）</option>
      </select>
      <p class="hint">为什么默认 Anthropic：托管别名 <code>ark-code-latest</code> 只在 Anthropic 口解析；OpenAI 口用该别名会 404。</p>

      <label>Base URL</label>
      <input id="base_url" value="${s.base_url}" placeholder="${ANTHROPIC_URL}" />
      <p class="hint">⚠️ 不要用 <code>/api/v3</code>（不走 Coding Plan 额度且会额外计费）。</p>

      <label>模型（model）</label>
      <input id="model" list="model_list" value="${s.model}" placeholder="ark-code-latest 或具体模型名" />
      <datalist id="model_list">
        ${models.map((m) => `<option value="${m}"></option>`).join("")}
      </datalist>
      <p class="hint"><code>ark-code-latest</code> 需在控制台「开通管理」选好生效模型；也可直接选具体模型名（如 deepseek-v4-pro、kimi-k2.7-code）。</p>

      <label>API Key（Bearer Token）${s.has_api_key ? "— 已保存，留空表示不修改" : ""}</label>
      <input id="api_key" type="password" placeholder="${s.has_api_key ? "•••••••• 已保存" : "粘贴已订阅 Coding Plan 的 API Key"}" />

      <div class="row">
        <div>
          <label>temperature</label>
          <input id="temperature" type="number" step="0.1" min="0" max="2" value="${s.temperature}" />
        </div>
        <div>
          <label>max_tokens（单次回复上限）</label>
          <input id="max_tokens" type="number" min="1" max="65536" value="${s.max_tokens}" />
        </div>
        <div>
          <label>最大并发</label>
          <input id="max_concurrency" type="number" min="1" max="16" value="${s.max_concurrency}" />
        </div>
        <div>
          <label>超时(秒)</label>
          <input id="timeout_secs" type="number" min="5" value="${s.timeout_secs}" />
        </div>
      </div>

      <div class="btn-row">
        <button class="primary" id="save">保存配置</button>
        <button class="ghost" id="test">测试连接</button>
      </div>
      <div class="toast" id="toast"></div>
      <p class="hint">密钥存储在应用本地配置（tauri-plugin-store），不会回传到界面，也不写入工作流文件。</p>
    </div>
  `;

  const toast = el.querySelector("#toast") as HTMLElement;
  const show = (msg: string, ok: boolean) => {
    toast.textContent = msg;
    toast.className = `toast show ${ok ? "ok" : "err"}`;
  };
  const val = (id: string) => (el.querySelector("#" + id) as HTMLInputElement).value;
  const baseUrlInput = el.querySelector("#base_url") as HTMLInputElement;
  const protocolSel = el.querySelector("#protocol") as HTMLSelectElement;

  // Switching protocol auto-fills the matching base_url (unless user customised).
  protocolSel.addEventListener("change", () => {
    const cur = baseUrlInput.value.trim();
    if (cur === ANTHROPIC_URL || cur === OPENAI_URL || cur === "") {
      baseUrlInput.value =
        protocolSel.value === "anthropic" ? ANTHROPIC_URL : OPENAI_URL;
    }
  });

  el.querySelector("#save")!.addEventListener("click", async () => {
    try {
      // Clamp max_tokens to a sane single-response ceiling. Users sometimes
      // enter the context-window size (e.g. 256000) here, which the gateway
      // rejects — max_tokens is the OUTPUT limit, not the context window.
      let maxTokens = parseInt(val("max_tokens"));
      let clampNote = "";
      if (!Number.isFinite(maxTokens) || maxTokens < 1) maxTokens = 4096;
      if (maxTokens > 65536) {
        maxTokens = 16384;
        clampNote = "（max_tokens 过大已自动调整为 16384）";
      }
      await api.saveSettings({
        base_url: val("base_url").trim(),
        model: val("model").trim(),
        protocol: protocolSel.value,
        api_key: val("api_key"), // empty => keep existing
        temperature: parseFloat(val("temperature")),
        max_tokens: maxTokens,
        max_concurrency: parseInt(val("max_concurrency")),
        timeout_secs: parseInt(val("timeout_secs")),
      });
      // Reflect any clamp in the input box too.
      (el.querySelector("#max_tokens") as HTMLInputElement).value = String(maxTokens);
      show("✅ 配置已保存" + clampNote, true);
    } catch (e) {
      show("保存失败：" + e, false);
    }
  });

  el.querySelector("#test")!.addEventListener("click", async () => {
    show("⏳ 正在测试连接…", true);
    try {
      const r = await api.testConnection();
      show("✅ " + r, true);
    } catch (e) {
      show("❌ " + e, false);
    }
  });
}
