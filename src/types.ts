// TypeScript mirrors of the Rust DTOs returned over the Tauri IPC boundary.

export interface ContextInfo {
  name: string;
  cluster: string;
  server: string;
  is_k3s: boolean;
}

export interface KubeStatus {
  connected: boolean;
  cluster_version: string | null;
  current_context: string | null;
  kubeconfig_path: string | null;
  context_count: number;
  contexts: ContextInfo[];
  error: string | null;
}

export interface PodRow {
  name: string;
  namespace: string;
  phase: string;
  ready: string;
  restarts: number;
  age: string;
  node: string;
  containers: string[];
}

export interface NodeRow {
  name: string;
  status: string;
  roles: string;
  version: string;
  age: string;
}

export interface TableData {
  headers: string[];
  rows: string[][];
}

export interface KubeconfigEntry {
  id: string;
  path: string;
  name: string;
  description: string | null;
  last_context: string | null;
}

export type ThemeMode = "Dark" | "Light" | "Custom";

export type ColorSchemeKey =
  | "Default"
  | "K3sPurple"
  | "KubernetesBlue"
  | "Emerald"
  | "Amber"
  | "Cyan"
  | "Rose"
  | "Slate";

/** Mirror of the Rust `AppState` persisted to settings.json. */
export interface AppState {
  kubeconfigs: KubeconfigEntry[];
  active_kubeconfig_id: string | null;
  registered_kubeconfigs: string[];
  active_kubeconfig_path: string | null;
  kubeconfig_path: string | null;
  default_namespace: string;
  theme_mode: ThemeMode;
  font_scale: number;
  log_level: string;
  custom_accent: [number, number, number] | null;
  color_scheme: ColorSchemeKey;
  last_kubeconfig_path: string | null;
  last_context: string | null;
  show_right_panel: boolean;
  auto_refresh_secs: number;
}

export interface ColorSchemeInfo {
  key: ColorSchemeKey;
  label: string;
  hex: string;
}

export interface LogEvent {
  kind: "header" | "line" | "error" | "ended";
  line: string;
}

/** Full detail for one selected resource (any kind). */
export interface ResourceDetail {
  kind: string;
  name: string;
  namespace: string | null;
  age: string;
  labels: [string, string][];
  annotations: [string, string][];
  manifest: string;
}

/** Frontend-only: the currently selected row across any list view. */
export interface Selection {
  /** Backend resource kind, e.g. "pods", "deployments". */
  kind: string;
  name: string;
  namespace: string | null;
  /** Projected [header, value] pairs from the table row (name excluded). */
  summary: [string, string][];
}
