const ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

export function escapeHtml(value: unknown): string {
  return String(value).replace(/[&<>"']/g, (char) => ESCAPES[char]);
}

export function errorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (
    /__TAURI|Cannot read properties of undefined \(reading ['"]invoke['"]\)|invoke is not a function/i.test(
      message,
    )
  ) {
    return "此功能需要在 Tauri 桌面应用中运行。";
  }
  return message;
}

export function setButtonBusy(
  button: HTMLButtonElement,
  busy: boolean,
  busyLabel: string,
): void {
  const idleLabel = button.dataset.idleLabel ?? button.textContent ?? "";
  button.dataset.idleLabel = idleLabel;
  button.disabled = busy;
  button.setAttribute("aria-busy", String(busy));
  button.textContent = busy ? busyLabel : idleLabel;
}
