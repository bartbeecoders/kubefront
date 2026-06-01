// Typed wrappers around the Tauri command surface. Every function here maps 1:1
// to a `#[tauri::command]` in src-tauri/src/commands.rs.

import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  AppState,
  ColorSchemeInfo,
  KubeStatus,
  LogEvent,
  NodeRow,
  PodRow,
  ResourceDetail,
  TableData,
} from "./types";

export const api = {
  getSettings: () => invoke<AppState>("get_settings"),
  saveSettings: (settings: AppState) => invoke<void>("save_settings", { settings }),
  resolvedAccent: () => invoke<string>("resolved_accent"),
  colorSchemes: () => invoke<ColorSchemeInfo[]>("color_schemes"),
  removeKubeconfig: (id: string) => invoke<AppState>("remove_kubeconfig", { id }),

  loadKubeconfig: (path: string | null) =>
    invoke<KubeStatus>("load_kubeconfig", { path }),
  setContext: (name: string) => invoke<KubeStatus>("set_context", { name }),
  getStatus: () => invoke<KubeStatus>("get_status"),
  connect: () => invoke<KubeStatus>("connect"),
  switchKubeconfig: (path: string) => invoke<KubeStatus>("switch_kubeconfig", { path }),

  listPods: (namespace: string | null) => invoke<PodRow[]>("list_pods", { namespace }),
  listNodes: () => invoke<NodeRow[]>("list_nodes"),
  listResource: (kind: string, namespace: string | null) =>
    invoke<TableData>("list_resource", { kind, namespace }),
  getResource: (kind: string, namespace: string | null, name: string) =>
    invoke<ResourceDetail>("get_resource", { kind, namespace, name }),

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
};
