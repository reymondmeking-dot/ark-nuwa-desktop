import "./styles.css";
import { renderSettings } from "./views/settings";
import { renderEditor } from "./views/editor";
import { renderRunner } from "./views/runner";
import { renderChat } from "./views/chat";

const views: Record<string, (el: HTMLElement) => void> = {
  settings: renderSettings,
  editor: renderEditor,
  runner: renderRunner,
  chat: renderChat,
};

function activate(name: string) {
  document.querySelectorAll(".nav-btn").forEach((b) =>
    b.classList.toggle("active", (b as HTMLElement).dataset.view === name)
  );
  document.querySelectorAll(".view").forEach((v) =>
    v.classList.toggle("active", v.id === `view-${name}`)
  );
  const el = document.getElementById(`view-${name}`);
  if (el) views[name](el as HTMLElement);
}

document.querySelectorAll(".nav-btn").forEach((btn) => {
  btn.addEventListener("click", () =>
    activate((btn as HTMLElement).dataset.view!)
  );
});

// Initial view.
activate("settings");
