import type { UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../api";
import { errorMessage, escapeHtml, setButtonBusy } from "../dom";

let unlisten: UnlistenFn | null = null;
let activeSession: string | null = null;

export async function renderChat(el: HTMLElement) {
  const sessions = await api.listSessions().catch(() => []);
  const storedSession = sessionStorage.getItem("active_session");
  activeSession = sessions.some((session) => session.id === storedSession)
    ? storedSession
    : (sessions[0]?.id ?? null);
  if (activeSession) sessionStorage.setItem("active_session", activeSession);
  else sessionStorage.removeItem("active_session");

  el.innerHTML = `
    <h1>蒸馏后智能体 · 对话</h1>
    <p class="sub">加载已蒸馏的视角 Skill 进行多轮对话。智能体遇到能力边界时会明确表达不确定性。</p>
    <div id="picker" class="session-picker" aria-label="选择智能体">${
      sessions.length
        ? sessions
            .map(
              (session) =>
                `<button class="pill ${session.id === activeSession ? "active" : ""}" type="button" data-id="${escapeHtml(session.id)}" aria-pressed="${session.id === activeSession}">${escapeHtml(session.title)}</button>`,
            )
            .join("")
        : '<p class="hint">还没有已蒸馏的智能体。请先到“运行”完成一次女娲蒸馏闭环。</p>'
    }</div>
    <div class="card chat-wrap" id="wrap" ${sessions.length ? "" : "hidden"}>
      <div class="messages" id="messages" role="log" aria-live="polite" aria-relevant="additions text"></div>
      <div class="chat-status hint" id="chat-status" role="status" aria-live="polite">${activeSession ? "已就绪" : ""}</div>
      <div class="chat-input">
        <label class="sr-only" for="input">消息</label>
        <textarea id="input" rows="2" placeholder="向蒸馏后的智能体提问…（Enter 发送，Shift+Enter 换行）"></textarea>
        <button class="primary" id="send" type="button">发送</button>
      </div>
    </div>
  `;

  const messages = el.querySelector("#messages") as HTMLElement;
  const input = el.querySelector("#input") as HTMLTextAreaElement;
  const sendButton = el.querySelector("#send") as HTMLButtonElement;
  const status = el.querySelector("#chat-status") as HTMLElement;
  const sessionButtons = Array.from(
    el.querySelectorAll<HTMLButtonElement>(".session-picker .pill"),
  );

  sessionButtons.forEach((button) => {
    button.addEventListener("click", () => {
      activeSession = button.dataset.id ?? null;
      if (activeSession) sessionStorage.setItem("active_session", activeSession);
      sessionButtons.forEach((candidate) => {
        const selected = candidate === button;
        candidate.classList.toggle("active", selected);
        candidate.setAttribute("aria-pressed", String(selected));
      });
      messages.replaceChildren();
      status.textContent = "已切换智能体";
      input.focus();
    });
  });

  if (unlisten) {
    unlisten();
    unlisten = null;
  }

  const replyState: { streamingElement: HTMLElement | null } = {
    streamingElement: null,
  };
  const currentStreamingElement = (): HTMLElement | null =>
    replyState.streamingElement;
  try {
    unlisten = await api.onChatEvent((event) => {
      if (event.session_id !== activeSession) return;
      if (!replyState.streamingElement) {
        replyState.streamingElement = appendMessage(messages, "assistant", "");
      }
      replyState.streamingElement.textContent += event.delta;
      messages.scrollTop = messages.scrollHeight;
    });
  } catch (error) {
    status.textContent = `流式监听不可用，将显示完整回复：${errorMessage(error)}`;
  }

  const setSending = (sending: boolean) => {
    setButtonBusy(sendButton, sending, "发送中…");
    input.disabled = sending;
    sessionButtons.forEach((button) => {
      button.disabled = sending;
    });
  };

  const send = async () => {
    const text = input.value.trim();
    if (!text || !activeSession || sendButton.disabled) return;
    const sessionId = activeSession;
    appendMessage(messages, "user", text);
    input.value = "";
    replyState.streamingElement = null;
    setSending(true);
    status.textContent = "智能体正在回复…";
    try {
      const response = await api.chatSend(sessionId, text);
      if (activeSession === sessionId) {
        const streamedReply = currentStreamingElement();
        if (!streamedReply) {
          replyState.streamingElement = appendMessage(messages, "assistant", response);
        } else if (response && streamedReply.textContent !== response) {
          streamedReply.textContent = response;
        }
      }
      status.textContent = "回复完成";
    } catch (error) {
      appendMessage(messages, "assistant", `出错：${errorMessage(error)}`);
      status.textContent = "回复失败";
    } finally {
      replyState.streamingElement = null;
      setSending(false);
      input.focus();
    }
  };

  sendButton.addEventListener("click", () => void send());
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      void send();
    }
  });
}

function appendMessage(
  container: HTMLElement,
  role: "user" | "assistant",
  text: string,
): HTMLElement {
  const message = document.createElement("div");
  message.className = `msg ${role}`;
  message.setAttribute("aria-label", role === "user" ? "你" : "智能体");
  message.textContent = text;
  container.append(message);
  container.scrollTop = container.scrollHeight;
  return message;
}
