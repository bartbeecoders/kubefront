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

/** How a connection reaches its cluster: a local kubeconfig or a remote backend. */
export type ConnMode = "Direct" | "Remote";

/** Orchestrator type for a connection (user-declared). */
export type ClusterType = "K3s" | "K8s" | "Aks";

/** Deployment environment a connection targets. */
export type Environment = "Dev" | "Val" | "Prod";

export interface KubeconfigEntry {
  id: string;
  path: string;
  name: string;
  description: string | null;
  last_context: string | null;
  /** Per-connection namespace; null/empty falls back to the global default_namespace. */
  namespace: string | null;
  /** "Direct" = local kubeconfig (today); "Remote" = a kubefront-backend endpoint. */
  mode: ConnMode;
  /** Backend base URL for Remote connections (e.g. https://host/site/connection). */
  endpoint: string | null;
  /** Optional PEM CA bundle path to trust for a Remote endpoint (OT internal CA). */
  ca_path: string | null;
  /** Skip TLS verification for a Remote endpoint (self-signed proxy). */
  insecure: boolean;
  // --- World view / inventory metadata (all optional) ---
  /** City this cluster lives in (map label). */
  city: string | null;
  /** Country this cluster lives in (drives the map position via geocoding). */
  country: string | null;
  /** Explicit latitude (decimal degrees); overrides the country centroid on the map. */
  latitude: number | null;
  /** Explicit longitude (decimal degrees). */
  longitude: number | null;
  /** Orchestrator type (K3S / K8S / AKS). */
  cluster_type: ClusterType | null;
  /** Manufacturing plant this cluster belongs to. */
  plant: string | null;
  /** Deployment environment (dev / val / prod). */
  environment: Environment | null;
}

/** Editable fields for `update_connection` (mirrors the Rust `ConnectionPatch`). */
export interface ConnectionPatch {
  name: string;
  description: string | null;
  namespace: string | null;
  /** Remote-only; ignored for Direct entries. */
  endpoint: string | null;
  ca_path: string | null;
  insecure: boolean;
  city: string | null;
  country: string | null;
  latitude: number | null;
  longitude: number | null;
  cluster_type: ClusterType | null;
  plant: string | null;
  environment: Environment | null;
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

/** Live health snapshot for one Dashboard cluster card. Null counts = no RBAC. */
export interface ClusterSummary {
  reachable: boolean;
  version: string | null;
  nodes_total: number | null;
  nodes_ready: number | null;
  pods_total: number | null;
  pods_running: number | null;
  namespaces: number | null;
  deployments: number | null;
  error: string | null;
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
