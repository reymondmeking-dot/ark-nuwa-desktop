import { api } from "../api";
import type { UnlistenFn } from "@tauri-apps/api/event";

let unlisten: UnlistenFn | null = null;
let activeSession: string | null = null;

export async function renderChat(el: HTMLElement) {
  const sessions = await api.listSessions().catch(() => []);
  activeSession =
    sessionStorage.getItem("active_session") ||
    (sessions[0]?.id ?? null);

  el.innerHTML = `
    <h1>蒸馏后智能体 · 对话</h1>
    <p class="sub">加载已蒸馏的视角 Skill 进行多轮对话。遇到未知问题，智能体会按其能力边界诚实表达不确定。</p>
    <div id="picker">${
      sessions.length
        ? sessions
            .map(
              (s) =>
                `<span class="pill ${s.id === activeSession ? "active" : ""}" data-id="${s.id}">${s.title}</span>`
            )
            .join("")
        : '<p class="hint">还没有已蒸馏的智能体。请到「运行」跑一次女娲蒸馏闭环，完成后会自动生成。</p>'
    }</div>
    <div class="card chat-wrap" id="wrap" style="${sessions.length ? "" : "display:none"}">
      <div class="messages" id="messages"></div>
      <div class="chat-input">
        <textarea id="input" placeholder="向蒸馏后的智能体提问…（Enter 发送，Shift+Enter 换行）"></textarea>
        <button class="primary" id="send">发送</button>
      </div>
    </div>
  `;

  const messages = el.querySelector("#messages") as HTMLElement;
  const input = el.querySelector("#input") as HTMLTextAreaElement;

  el.querySelectorAll(".pill").forEach((p) =>
    p.addEventListener("click", () => {
      activeSession = (p as HTMLElement).dataset.id!;
      sessionStorage.setItem("active_session", activeSession);
      el.querySelectorAll(".pill").forEach((x) =>
        x.classList.toggle("active", x === p)
      );
      messages.innerHTML = "";
    })
  );

  if (unlisten) { unlisten(); unlisten = null; }
  let streamingEl: HTMLElement | null = null;
  unlisten = await api.onChatEvent((e) => {
    if (e.session_id !== activeSession) return;
    if (!streamingEl) streamingEl = appendMsg(messages, "assistant", "");
    streamingEl.textContent += e.delta;
    messages.scrollTop = messages.scrollHeight;
  });

  const send = async () => {
    const text = input.value.trim();
    if (!text || !activeSession) return;
    appendMsg(messages, "user", text);
    input.value = "";
    streamingEl = null;
    try {
      await api.chatSend(activeSession, text);
    } catch (e) {
      appendMsg(messages, "assistant", "⚠️ 出错：" + e);
    }
    streamingEl = null;
  };

  el.querySelector("#send")!.addEventListener("click", send);
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      send();
    }
  });
}

function appendMsg(
  container: HTMLElement,
  role: "user" | "assistant",
  text: string
): HTMLElement {
  const div = document.createElement("div");
  div.className = `msg ${role}`;
  div.textContent = text;
  container.appendChild(div);
  container.scrollTop = container.scrollHeight;
  return div;
}
