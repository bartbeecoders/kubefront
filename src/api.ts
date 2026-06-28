// Typed wrappers around the Tauri command surface. Every function here maps 1:1
// to a `#[tauri::command]` in src-tauri/src/commands.rs.

import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  AksCluster,
  AppState,
  AzureStatus,
  AzureSubscription,
  ClusterSummary,
  ColorSchemeInfo,
  ConnectionPatch,
  ContextInfo,
  KubeStatus,
  LogEvent,
  NodeRow,
  PodRow,
  ResourceDetail,
  TableData,
  TerminalEvent,
} from "./types";

export const api = {
  getSettings: () => invoke<AppState>("get_settings"),
  saveSettings: (settings: AppState) => invoke<void>("save_settings", { settings }),
  resolvedAccent: () => invoke<string>("resolved_accent"),
  colorSchemes: () => invoke<ColorSchemeInfo[]>("color_schemes"),
  logPath: () => invoke<string | null>("log_path"),
  removeKubeconfig: (id: string) => invoke<AppState>("remove_kubeconfig", { id }),

  // --- Remote backend connections ---
  /** Register (or update) a remote backend connection; returns updated settings. */
  addRemoteConnection: (
    name: string,
    endpoint: string,
    caPath: string | null,
    insecure: boolean,
  ) => invoke<AppState>("add_remote_connection", { name, endpoint, caPath, insecure }),
  /** Edit an existing connection (Direct or Remote) in place; returns updated settings.
   *  endpoint/ca_path/insecure in the patch are ignored for Direct connections. */
  updateConnection: (id: string, patch: ConnectionPatch) =>
    invoke<AppState>("update_connection", { id, patch }),
  /** Remove any connection (Direct or Remote) by id; returns updated settings. */
  removeConnection: (id: string) => invoke<AppState>("remove_connection", { id }),
  /** Probe a remote endpoint without making it active (Settings "Test" button). */
  testRemoteConnection: (endpoint: string, caPath: string | null, insecure: boolean) =>
    invoke<KubeStatus>("test_remote_connection", { endpoint, caPath, insecure }),
  /** Dashboard remote-card health probe for one remote connection. */
  remoteSummary: (connectionId: string) =>
    invoke<ClusterSummary>("remote_summary", { connectionId }),
  /** Make a connection active and connect to it (dispatches by its mode). */
  selectConnection: (id: string) => invoke<KubeStatus>("select_connection", { id }),

  // --- Azure AKS ---
  /** Probe the local Azure CLI: installed? logged in? (preflight for the AKS wizard). */
  azureStatus: () => invoke<AzureStatus>("azure_status"),
  /** List the Azure subscriptions visible to the signed-in `az` account. */
  azureSubscriptions: () => invoke<AzureSubscription[]>("azure_subscriptions"),
  /** List the AKS clusters within a subscription. */
  azureAksClusters: (subscriptionId: string) =>
    invoke<AksCluster[]>("azure_aks_clusters", { subscriptionId }),
  /** Fetch AAD credentials for an AKS cluster and register it as a Direct connection. */
  addAksConnection: (
    subscriptionId: string,
    resourceGroup: string,
    clusterName: string,
    displayName: string | null,
  ) =>
    invoke<AppState>("add_aks_connection", {
      subscriptionId,
      resourceGroup,
      clusterName,
      displayName,
    }),

  loadKubeconfig: (path: string | null) =>
    invoke<KubeStatus>("load_kubeconfig", { path }),
  setContext: (name: string) => invoke<KubeStatus>("set_context", { name }),
  getStatus: () => invoke<KubeStatus>("get_status"),
  connect: () => invoke<KubeStatus>("connect"),
  switchKubeconfig: (path: string) => invoke<KubeStatus>("switch_kubeconfig", { path }),

  /** Dashboard: load + select a specific context in a kubeconfig and connect. */
  openCluster: (path: string | null, context: string) =>
    invoke<KubeStatus>("open_cluster", { path, context }),
  /** Dashboard: contexts of a kubeconfig file without touching the connection. */
  kubeconfigContexts: (path: string | null) =>
    invoke<ContextInfo[]>("kubeconfig_contexts", { path }),
  /** Dashboard: probe one cluster's health with a short-lived client. */
  clusterSummary: (path: string | null, context: string, namespace: string | null) =>
    invoke<ClusterSummary>("cluster_summary", { path, context, namespace }),

  listPods: (namespace: string | null) => invoke<PodRow[]>("list_pods", { namespace }),
  listNodes: () => invoke<NodeRow[]>("list_nodes"),
  listResource: (kind: string, namespace: string | null) =>
    invoke<TableData>("list_resource", { kind, namespace }),
  getResource: (kind: string, namespace: string | null, name: string) =>
    invoke<ResourceDetail>("get_resource", { kind, namespace, name }),
  deleteResource: (kind: string, namespace: string | null, name: string) =>
    invoke<void>("delete_resource", { kind, namespace, name }),
  /** Rollout-restart for workloads; for a pod this deletes it (controller recreates). */
  restartResource: (kind: string, namespace: string | null, name: string) =>
    invoke<void>("restart_resource", { kind, namespace, name }),
  /** Replace a ConfigMap's `data` map (keys absent from `data` are removed). */
  updateConfigmap: (namespace: string, name: string, data: Record<string, string>) =>
    invoke<void>("update_configmap", { namespace, name, data }),
  /** kubectl-describe-style text report for a pod (status, containers, events). */
  describePod: (namespace: string, name: string) =>
    invoke<string>("describe_pod", { namespace, name }),

  /** Start a live log stream. The returned id can be passed to `stopLogs`. */
  streamLogs: (
    namespace: string,
    pod: string,
    container: string | null,
    onEvent: (e: LogEvent) => void,
  ): Promise<number> => {
    const channel = new Channel<LogEvent>();
    channel.onmessage = onEvent;
    return invoke<number>("stream_logs", {
      namespace,
      pod,
      container,
      onEvent: channel,
    });
  },
  stopLogs: (id: number) => invoke<void>("stop_logs", { id }),

  // --- Embedded terminal (PTY) ---
  /** Open a PTY-backed shell scoped to the active cluster. `onEvent` receives
   *  output bytes + the exit event; returns the terminal id for the other calls. */
  openTerminal: (
    cols: number,
    rows: number,
    onEvent: (e: TerminalEvent) => void,
  ): Promise<number> => {
    const channel = new Channel<TerminalEvent>();
    channel.onmessage = onEvent;
    return invoke<number>("terminal_open", { onEvent: channel, cols, rows });
  },
  /** Send keystrokes (xterm onData) to the shell. */
  writeTerminal: (id: number, data: string) => invoke<void>("terminal_write", { id, data }),
  /** Resize the PTY to match the xterm viewport. */
  resizeTerminal: (id: number, cols: number, rows: number) =>
    invoke<void>("terminal_resize", { id, cols, rows }),
  /** Kill the shell and free the session. */
  closeTerminal: (id: number) => invoke<void>("terminal_close", { id }),
};
