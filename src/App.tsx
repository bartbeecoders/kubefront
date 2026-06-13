import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { api } from "./api";
import type {
  AppState,
  ColorSchemeInfo,
  KubeconfigEntry,
  KubeStatus,
  LogEvent,
  NodeRow,
  PodRow,
  Selection,
  TableData,
} from "./types";
import { TABLE_VIEWS, kindLabel, type ViewKey } from "./views";

import { TopBar } from "./components/TopBar";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { DetailPanel } from "./components/DetailPanel";
import { LogWindow, type LogWindowState } from "./components/LogWindow";
import { ConfirmDialog, type ConfirmRequest } from "./components/ConfirmDialog";
import { ConfigMapEditor, type ConfigMapEditRequest } from "./components/ConfigMapEditor";
import { ConnectionEditor } from "./components/ConnectionEditor";
import { TextViewModal } from "./components/TextViewModal";

import { DashboardView } from "./views/Dashboard";
import { PodsView } from "./views/Pods";
import { NodesView } from "./views/Nodes";
import { ClustersView } from "./views/Clusters";
import { MonitoringView } from "./views/Monitoring";
import { LoggingView } from "./views/Logging";
import { TableView } from "./views/TableView";
import { SettingsView } from "./views/Settings";

const EMPTY_STATUS: KubeStatus = {
  connected: false,
  cluster_version: null,
  current_context: null,
  kubeconfig_path: null,
  context_count: 0,
  contexts: [],
  error: null,
};

function defaultSettings(): AppState {
  return {
    kubeconfigs: [],
    active_kubeconfig_id: null,
    registered_kubeconfigs: [],
    active_kubeconfig_path: null,
    kubeconfig_path: null,
    default_namespace: "All",
    theme_mode: "Dark",
    font_scale: 1.0,
    log_level: "info",
    custom_accent: null,
    color_scheme: "Default",
    last_kubeconfig_path: null,
    last_context: null,
    show_right_panel: true,
    auto_refresh_secs: 5,
  };
}

const rgbToHex = (rgb: [number, number, number]) =>
  "#" + rgb.map((c) => c.toString(16).padStart(2, "0")).join("");

function applyTheme(s: AppState, schemes: ColorSchemeInfo[]) {
  const root = document.documentElement;
  root.dataset.theme = s.theme_mode === "Light" ? "light" : "dark";
  root.style.setProperty("--font-scale", String(s.font_scale));
  const accent = s.custom_accent
    ? rgbToHex(s.custom_accent)
    : schemes.find((x) => x.key === s.color_scheme)?.hex ?? "#326ce5";
  root.style.setProperty("--accent", accent);
}

/** Resource kinds that must be fetched for a given view. */
function kindsForView(view: ViewKey): string[] {
  if (view === "monitoring") return ["namespaces", "deployments", "services"];
  return TABLE_VIEWS[view]?.sections.map((s) => s.kind) ?? [];
}

async function pickKubeconfig(): Promise<string | null> {
  const res = await open({
    multiple: false,
    directory: false,
    filters: [
      { name: "Kubeconfig", extensions: ["yaml", "yml", "config"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  return typeof res === "string" ? res : null;
}

export default function App() {
  const [settings, setSettings] = useState<AppState | null>(null);
  const [schemes, setSchemes] = useState<ColorSchemeInfo[]>([]);
  const [status, setStatus] = useState<KubeStatus>(EMPTY_STATUS);
  const [connecting, setConnecting] = useState(false);

  const [view, setView] = useState<ViewKey>("dashboard");
  const [pods, setPods] = useState<PodRow[]>([]);
  const [nodes, setNodes] = useState<NodeRow[]>([]);
  const [tables, setTables] = useState<Record<string, TableData>>({});
  const [dataError, setDataError] = useState<string | null>(null);

  const [selected, setSelected] = useState<Selection | null>(null);
  const [podFilter, setPodFilter] = useState("");
  const [podNsFilter, setPodNsFilter] = useState("All");

  const [logWins, setLogWins] = useState<LogWindowState[]>([]);
  const nextWinId = useRef(1);

  const [confirm, setConfirm] = useState<ConfirmRequest | null>(null);
  const [editConn, setEditConn] = useState<KubeconfigEntry | null>(null);
  const [editCm, setEditCm] = useState<ConfigMapEditRequest | null>(null);
  // Bumped to force the DetailPanel to re-fetch its manifest after an edit.
  const [detailReloadKey, setDetailReloadKey] = useState(0);
  const [describe, setDescribe] = useState<{
    namespace: string;
    name: string;
    text: string;
    loading: boolean;
    error: string | null;
  } | null>(null);

  // ---- bootstrap ----
  useEffect(() => {
    (async () => {
      const [s, sc] = await Promise.all([api.getSettings(), api.colorSchemes()]);
      setSettings(s);
      setSchemes(sc);
      applyTheme(s, sc);
      // Backend is the single source of truth for the resolved accent on first load.
      try {
        document.documentElement.style.setProperty("--accent", await api.resolvedAccent());
      } catch {
        /* ignore */
      }
      setPodNsFilter(s.default_namespace || "All");

      let st = await api.getStatus();
      setStatus(st);
      if (st.current_context && !st.connected) {
        setConnecting(true);
        try {
          st = await api.connect();
          setStatus(st);
        } finally {
          setConnecting(false);
        }
      }
    })().catch((e) => console.error("bootstrap failed", e));
  }, []);

  // The namespace that scopes resource lists: the active connection's own
  // namespace if set, otherwise the global default. Lets each kubeconfig keep
  // its own scope (e.g. a deploy user that may only list one namespace).
  const activeEntry = settings?.kubeconfigs.find(
    (e) => e.id === settings.active_kubeconfig_id,
  );
  const effectiveNs =
    activeEntry?.namespace?.trim() || settings?.default_namespace || "All";

  // ---- data refresh ----
  // Fetch errors are collected and SHOWN, never swallowed — an RBAC 403 on a
  // cluster-wide list (common for namespace-scoped users with namespace "All")
  // must not render as a silently empty table.
  const refresh = useCallback(async () => {
    if (!settings) return;
    const ns = effectiveNs;
    const errors: string[] = [];
    if (view === "pods" || view === "logging" || view === "monitoring") {
      try {
        setPods(await api.listPods(ns));
      } catch (e) {
        errors.push(String(e));
      }
    }
    if (view === "nodes" || view === "monitoring") {
      try {
        setNodes(await api.listNodes());
      } catch (e) {
        errors.push(String(e));
      }
    }
    const kinds = kindsForView(view);
    if (kinds.length) {
      const results = await Promise.all(
        kinds.map((k) =>
          api
            .listResource(k, ns)
            .then((d) => [k, d] as const)
            .catch((e) => {
              errors.push(String(e));
              return [k, { headers: [], rows: [] } as TableData] as const;
            }),
        ),
      );
      setTables((prev) => ({ ...prev, ...Object.fromEntries(results) }));
    }
    const unique = [...new Set(errors)];
    setDataError(
      unique.length
        ? unique[0] + (unique.length > 1 ? ` (+${unique.length - 1} more)` : "")
        : null,
    );
  }, [settings, view, effectiveNs]);

  const refreshRef = useRef(refresh);
  useEffect(() => {
    refreshRef.current = refresh;
  }, [refresh]);

  // Immediate refresh on view change, new connection, or namespace scope change.
  useEffect(() => {
    if (status.connected) refreshRef.current();
  }, [view, status.connected, effectiveNs]);

  // Auto-refresh timer.
  useEffect(() => {
    if (!settings) return;
    const secs = Math.max(1, settings.auto_refresh_secs);
    const id = setInterval(() => {
      if (status.connected) refreshRef.current();
    }, secs * 1000);
    return () => clearInterval(id);
  }, [settings?.auto_refresh_secs, status.connected]);

  // ---- connection helpers ----
  const cancelAllLogs = useCallback(() => {
    setLogWins((wins) => {
      for (const w of wins) if (w.streamId != null) api.stopLogs(w.streamId);
      return [];
    });
  }, []);

  const doConnect = useCallback(async () => {
    cancelAllLogs();
    setConnecting(true);
    try {
      setStatus(await api.connect());
    } finally {
      setConnecting(false);
    }
  }, [cancelAllLogs]);

  async function selectContext(name: string) {
    setStatus(await api.setContext(name));
    await doConnect();
  }

  async function loadFromFile() {
    const path = await pickKubeconfig();
    if (!path) return;
    cancelAllLogs();
    setConnecting(true);
    try {
      setStatus(await api.switchKubeconfig(path));
      setSettings(await api.getSettings());
    } finally {
      setConnecting(false);
    }
  }

  async function loadDefault(path: string | null) {
    cancelAllLogs();
    const st = await api.loadKubeconfig(path);
    setStatus(st);
    await doConnect();
  }

  // ---- settings handlers ----
  function patchSettings(patch: Partial<AppState>) {
    setSettings((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...patch };
      api.saveSettings(next).catch(() => {});
      applyTheme(next, schemes);
      return next;
    });
  }

  async function applySettings() {
    if (!settings) return;
    cancelAllLogs();
    if (settings.kubeconfig_path) {
      setStatus(await api.loadKubeconfig(settings.kubeconfig_path));
    }
    await doConnect();
  }

  async function resetSettings() {
    const def = defaultSettings();
    setSettings(def);
    applyTheme(def, schemes);
    await api.saveSettings(def);
    await loadDefault(null);
  }

  async function removeKubeconfig(id: string) {
    setSettings(await api.removeConnection(id));
  }

  /** Open the edit modal for a connection (Direct or Remote). */
  function editConnection(id: string) {
    const entry = settings?.kubeconfigs.find((k) => k.id === id);
    if (entry) setEditConn(entry);
  }

  /** Make any connection (Direct or Remote) active and connect to it. */
  async function selectConnection(id: string) {
    cancelAllLogs();
    setConnecting(true);
    try {
      setStatus(await api.selectConnection(id));
      setSettings(await api.getSettings());
    } finally {
      setConnecting(false);
    }
  }

  /** Register a remote backend connection (Settings "Add remote"). */
  async function addRemote(
    name: string,
    endpoint: string,
    caPath: string | null,
    insecure: boolean,
  ) {
    setSettings(await api.addRemoteConnection(name, endpoint, caPath, insecure));
  }

  /** Dashboard card click: connect to that cluster, then open its details page. */
  async function openCluster(path: string | null, context: string) {
    cancelAllLogs();
    setConnecting(true);
    try {
      const st = await api.openCluster(path, context);
      setStatus(st);
      setSettings(await api.getSettings());
      if (st.connected) {
        setSelected(null);
        setView("monitoring");
      }
    } finally {
      setConnecting(false);
    }
  }

  // ---- resource actions (delete / restart, with confirmation) ----
  function fullName(namespace: string | null, name: string) {
    return namespace ? `${namespace}/${name}` : name;
  }

  function clearIfSelected(kind: string, namespace: string | null, name: string) {
    setSelected((sel) =>
      sel && sel.kind === kind && sel.name === name && (sel.namespace ?? null) === namespace
        ? null
        : sel,
    );
  }

  function requestDelete(kind: string, namespace: string | null, name: string) {
    setConfirm({
      title: `Delete ${kindLabel(kind)}`,
      message: `Delete ${kindLabel(kind)} "${fullName(namespace, name)}"? This cannot be undone.`,
      confirmLabel: "Delete",
      danger: true,
      action: async () => {
        await api.deleteResource(kind, namespace, name);
        clearIfSelected(kind, namespace, name);
        refreshRef.current();
      },
    });
  }

  function requestRestart(kind: string, namespace: string | null, name: string) {
    const isPod = kind === "pods";
    setConfirm({
      title: `Restart ${kindLabel(kind)}`,
      message: isPod
        ? `Restart pod "${fullName(namespace, name)}"? The pod is deleted and its controller recreates it — a standalone pod will NOT come back.`
        : `Restart ${kindLabel(kind)} "${fullName(namespace, name)}"? A rolling restart replaces all of its pods.`,
      confirmLabel: "Restart",
      action: async () => {
        await api.restartResource(kind, namespace, name);
        if (isPod) clearIfSelected(kind, namespace, name);
        refreshRef.current();
      },
    });
  }

  /** Open the ConfigMap editor, seeding it with the live `data` map. */
  async function requestEditConfigmap(
    kind: string,
    namespace: string | null,
    name: string,
  ) {
    if (kind !== "configmaps" || !namespace) return;
    try {
      const detail = await api.getResource(kind, namespace, name);
      const data = (JSON.parse(detail.manifest)?.data ?? {}) as Record<string, string>;
      setEditCm({ namespace, name, data });
    } catch (e) {
      setDataError(String(e));
    }
  }

  /** Fetch and show a kubectl-describe-style report for a pod. */
  async function openDescribe(namespace: string, name: string) {
    setDescribe({ namespace, name, text: "", loading: true, error: null });
    try {
      const text = await api.describePod(namespace, name);
      setDescribe({ namespace, name, text, loading: false, error: null });
    } catch (e) {
      setDescribe({ namespace, name, text: "", loading: false, error: String(e) });
    }
  }

  // ---- logs ----
  function onLogEvent(winId: number, e: LogEvent) {
    if (e.kind === "ended") return;
    setLogWins((wins) =>
      wins.map((w) => {
        if (w.id !== winId) return w;
        const err = e.kind === "error";
        const text = err ? `[ERROR] ${e.line}` : e.line;
        let lines = [...w.lines, { text, err }];
        if (lines.length > 5000) lines = lines.slice(lines.length - 4000);
        return { ...w, lines };
      }),
    );
  }

  async function openLogs(pod: PodRow) {
    const id = nextWinId.current++;
    const container = pod.containers[0] ?? null;
    const win: LogWindowState = {
      id,
      streamId: null,
      pod: pod.name,
      namespace: pod.namespace,
      containers: pod.containers,
      selectedContainer: container,
      lines: [],
      follow: true,
      filter: "",
    };
    setLogWins((w) => [...w, win]);
    try {
      const streamId = await api.streamLogs(pod.namespace, pod.name, container, (e) =>
        onLogEvent(id, e),
      );
      setLogWins((w) => w.map((x) => (x.id === id ? { ...x, streamId } : x)));
    } catch (e) {
      onLogEvent(id, { kind: "error", line: String(e) });
    }
  }

  function closeLog(id: number) {
    setLogWins((wins) => {
      const w = wins.find((x) => x.id === id);
      if (w?.streamId != null) api.stopLogs(w.streamId);
      return wins.filter((x) => x.id !== id);
    });
  }

  function patchLog(id: number, patch: Partial<LogWindowState>) {
    setLogWins((wins) => wins.map((w) => (w.id === id ? { ...w, ...patch } : w)));
  }

  async function changeContainer(id: number, container: string) {
    let target: LogWindowState | undefined;
    setLogWins((wins) => {
      target = wins.find((x) => x.id === id);
      if (target?.streamId != null) api.stopLogs(target.streamId);
      return wins.map((w) =>
        w.id === id ? { ...w, selectedContainer: container, lines: [], streamId: null } : w,
      );
    });
    if (!target) return;
    try {
      const streamId = await api.streamLogs(target.namespace, target.pod, container, (e) =>
        onLogEvent(id, e),
      );
      setLogWins((w) => w.map((x) => (x.id === id ? { ...x, streamId } : x)));
    } catch (e) {
      onLogEvent(id, { kind: "error", line: String(e) });
    }
  }

  // ---- render ----
  if (!settings) {
    return <div style={{ padding: 40, color: "var(--text-dim)" }}>Loading KubeFront…</div>;
  }

  const showDetail = view === "pods" || view === "nodes" || !!TABLE_VIEWS[view];

  function selectView(v: ViewKey) {
    setSelected(null);
    setView(v);
  }

  return (
    <div className="app">
      <TopBar
        status={status}
        connecting={connecting}
        onLoadFile={loadFromFile}
        onUseDefault={() => loadDefault(null)}
        onUseK3s={() => loadDefault("/etc/rancher/k3s/k3s.yaml")}
        onSelectContext={selectContext}
        onReconnect={doConnect}
        onRefresh={() => refreshRef.current()}
        autoRefreshSecs={settings.auto_refresh_secs}
      />

      {status.error && (
        <div className="error-banner">
          ⚠ {status.error}
          <div className="tip">
            If kubectl works in this terminal, the issue is likely network reachability or TLS from
            the app process. Run with RUST_LOG=debug for details.
          </div>
        </div>
      )}

      {!status.error && status.connected && dataError && (
        <div className="error-banner">
          ⚠ Failed to fetch resources: {dataError}
          <div className="tip">
            If this is a 403/Forbidden, your user is likely namespace-scoped — set the default
            namespace in Settings to a namespace you have access to instead of "All".
          </div>
        </div>
      )}

      <div className={`body-row${showDetail ? " with-detail" : ""}`}>
        <Sidebar active={view} onSelect={selectView} />

        <main className="content">{renderView()}</main>

        {showDetail && (
          <DetailPanel
            selected={selected}
            pods={pods}
            reloadKey={detailReloadKey}
            onOpenLogs={openLogs}
            onDelete={requestDelete}
            onRestart={requestRestart}
            onEdit={requestEditConfigmap}
            onDescribe={openDescribe}
          />
        )}
      </div>

      <StatusBar status={status} settings={settings} podCount={pods.length} />

      {logWins.map((w, i) => (
        <LogWindow
          key={w.id}
          win={w}
          index={i}
          onClose={closeLog}
          onPatch={patchLog}
          onChangeContainer={changeContainer}
        />
      ))}

      {confirm && <ConfirmDialog req={confirm} onClose={() => setConfirm(null)} />}

      {editConn && (
        <ConnectionEditor
          entry={editConn}
          onClose={() => setEditConn(null)}
          onSaved={async () => setSettings(await api.getSettings())}
        />
      )}

      {editCm && (
        <ConfigMapEditor
          req={editCm}
          onClose={() => setEditCm(null)}
          onSaved={() => {
            setDetailReloadKey((k) => k + 1);
            refreshRef.current();
          }}
        />
      )}

      {describe && (
        <TextViewModal
          title="Describe Pod"
          subtitle={`${describe.namespace}/${describe.name}`}
          text={describe.text}
          loading={describe.loading}
          error={describe.error}
          onClose={() => setDescribe(null)}
        />
      )}
    </div>
  );

  function renderView() {
    switch (view) {
      case "dashboard":
        return (
          <DashboardView
            settings={settings!}
            status={status}
            onOpen={openCluster}
            onSelectRemote={selectConnection}
          />
        );
      case "clusters":
        return <ClustersView status={status} onConnect={selectContext} />;
      case "nodes":
        return <NodesView nodes={nodes} selected={selected} onSelect={setSelected} />;
      case "pods":
        return (
          <PodsView
            pods={pods}
            filter={podFilter}
            setFilter={setPodFilter}
            nsFilter={podNsFilter}
            setNsFilter={setPodNsFilter}
            selected={selected}
            onSelect={setSelected}
            onOpenLogs={openLogs}
            onDelete={requestDelete}
            onRestart={requestRestart}
          />
        );
      case "monitoring":
        return (
          <MonitoringView connected={status.connected} pods={pods} nodes={nodes} tables={tables} />
        );
      case "logging":
        return <LoggingView pods={pods} onOpenLogs={openLogs} />;
      case "settings":
        return (
          <SettingsView
            settings={settings!}
            schemes={schemes}
            onChange={patchSettings}
            onApply={applySettings}
            onReset={resetSettings}
            onBrowse={async () => {
              const p = await pickKubeconfig();
              if (p) patchSettings({ kubeconfig_path: p });
            }}
            onAddKubeconfig={loadFromFile}
            onSelect={selectConnection}
            onAddRemote={addRemote}
            onEdit={editConnection}
            onRemove={removeKubeconfig}
            onClose={() => selectView("pods")}
          />
        );
      default: {
        const def = TABLE_VIEWS[view];
        if (def)
          return (
            <TableView
              def={def}
              tables={tables}
              selected={selected}
              onSelect={setSelected}
              onDelete={requestDelete}
              onRestart={requestRestart}
            />
          );
        return <div className="empty">Unknown view.</div>;
      }
    }
  }
}
