// Thin typed wrapper over the Tauri command surface.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface SettingsView {
  base_url: string;
  model: string;
  protocol: string;
  temperature: number;
  max_tokens: number;
  max_concurrency: number;
  timeout_secs: number;
  has_api_key: boolean;
}

export interface SettingsPayload {
  base_url: string;
  model: string;
  protocol: string;
  temperature: number;
  max_tokens: number;
  max_concurrency: number;
  timeout_secs: number;
  api_key: string; // empty string = keep existing
}

export type RunEvent =
  | { kind: "node_status"; node: string; status: NodeStatus }
  | { kind: "node_chunk"; node: string; delta: string }
  | { kind: "node_output"; node: string; output: string }
  | { kind: "loop_back"; from: string; to: string; attempt: number }
  | { kind: "log"; message: string }
  | { kind: "finished"; ok: boolean };

export type NodeStatus =
  | "pending" | "running" | "done" | "failed" | "retrying" | "skipped";

export const api = {
  getSettings: () => invoke<SettingsView>("get_settings"),
  saveSettings: (settings: SettingsPayload) =>
    invoke<void>("save_settings", { settings }),
  testConnection: () => invoke<string>("test_connection"),

  defaultWorkflowYaml: () => invoke<string>("default_workflow_yaml"),
  codingModels: () => invoke<string[]>("coding_models"),
  validateWorkflow: (source: string) =>
    invoke<{ name: string; node_count: number; layers: string[][] }>(
      "validate_workflow",
      { source }
    ),
  runWorkflow: (source: string, vars: Record<string, string>) =>
    invoke<{ outputs: Record<string, string>; skill: string | null; session_id: string | null }>(
      "run_workflow",
      { source, vars }
    ),

  listWorkflows: () =>
    invoke<{ filename: string; name: string }[]>("list_workflows"),
  saveWorkflow: (name: string, source: string) =>
    invoke<string>("save_workflow", { name, source }),
  loadWorkflow: (filename: string) =>
    invoke<string>("load_workflow", { filename }),
  deleteWorkflow: (filename: string) =>
    invoke<void>("delete_workflow", { filename }),

  listSessions: () => invoke<{ id: string; title: string }[]>("list_sessions"),
  createSession: (title: string, skill_markdown: string) =>
    invoke<string>("create_session", { title, skillMarkdown: skill_markdown }),
  chatSend: (session_id: string, message: string) =>
    invoke<string>("chat_send", { sessionId: session_id, message }),

  onWorkflowEvent: (cb: (e: RunEvent) => void): Promise<UnlistenFn> =>
    listen<RunEvent>("workflow://event", (e) => cb(e.payload)),
  onChatEvent: (
    cb: (e: { session_id: string; delta: string }) => void
  ): Promise<UnlistenFn> =>
    listen<{ session_id: string; delta: string }>("chat://event", (e) =>
      cb(e.payload)
    ),
};
