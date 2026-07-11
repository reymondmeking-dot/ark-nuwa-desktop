import "./styles.css";
import { renderSettings } from "./views/settings";
import { renderEditor } from "./views/editor";
import { renderRunner } from "./views/runner";
import { renderChat } from "./views/chat";
import { errorMessage } from "./dom";

type ViewName = "settings" | "editor" | "runner" | "chat";

const views: Record<ViewName, (el: HTMLElement) => Promise<void>> = {
  settings: renderSettings,
  editor: renderEditor,
  runner: renderRunner,
  chat: renderChat,
};

let activeView: ViewName | null = null;

function isViewName(value: string | undefined): value is ViewName {
  return value !== undefined && value in views;
}

async function activate(name: ViewName) {
  if (activeView === name) return;
  activeView = name;

  document.querySelectorAll<HTMLButtonElement>(".nav-btn").forEach((button) => {
    const selected = button.dataset.view === name;
    button.classList.toggle("active", selected);
    if (selected) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  document.querySelectorAll<HTMLElement>(".view").forEach((view) => {
    const selected = view.id === `view-${name}`;
    view.classList.toggle("active", selected);
    view.hidden = !selected;
  });
  const el = document.getElementById(`view-${name}`);
  if (!el) return;

  el.setAttribute("aria-busy", "true");
  try {
    await views[name](el);
  } catch (error) {
    el.replaceChildren();
    const message = document.createElement("div");
    message.className = "toast show err";
    message.setAttribute("role", "alert");
    message.textContent = `页面加载失败：${errorMessage(error)}`;
    el.append(message);
  } finally {
    el.removeAttribute("aria-busy");
  }
}

document.querySelectorAll<HTMLButtonElement>(".nav-btn").forEach((button) => {
  button.addEventListener("click", () => {
    if (isViewName(button.dataset.view)) void activate(button.dataset.view);
  });
});

// Initial view.
void activate("settings");
