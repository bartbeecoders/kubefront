import type { KubeStatus } from "../types";

interface Props {
  status: KubeStatus;
  connecting: boolean;
  onLoadFile: () => void;
  onUseDefault: () => void;
  onUseK3s: () => void;
  onSelectContext: (name: string) => void;
  onReconnect: () => void;
  onRefresh: () => void;
  onOpenTerminal: () => void;
  autoRefreshSecs: number;
}

export function TopBar(props: Props) {
  const { status, connecting } = props;
  const current = status.contexts.find((c) => c.name === status.current_context);
  // A connected connection whose "kubeconfig path" is a URL is a remote backend.
  const isRemote =
    !!status.kubeconfig_path && /^https?:\/\//.test(status.kubeconfig_path);

  return (
    <header className="topbar">
      <div className="brand">
        <span className="logo">KubeFront</span>
        <span className="sub">K3S &amp; Kubernetes</span>
      </div>

      <div className="sep" />

      <button className="btn sm" onClick={props.onLoadFile}>
        Load kubeconfig…
      </button>
      <button className="btn sm" onClick={props.onUseDefault}>
        Use default
      </button>
      <button className="btn sm" onClick={props.onUseK3s}>
        K3S default
      </button>

      <div className="sep" />

      {isRemote ? (
        <>
          <span className="ctx-label">Remote</span>
          <span className="mono" title={status.kubeconfig_path ?? ""}>
            {status.current_context ?? "backend"}
          </span>
          <span className="pill pill-remote">Remote</span>
        </>
      ) : (
        <>
          <span className="ctx-label">Context</span>
          <select
            className="select"
            value={status.current_context ?? ""}
            onChange={(e) => props.onSelectContext(e.target.value)}
          >
            {!status.current_context && <option value="">Select context…</option>}
            {status.contexts.map((c) => (
              <option key={c.name} value={c.name}>
                {c.name}
                {c.is_k3s ? "  🟣 K3S" : ""} ({c.cluster})
              </option>
            ))}
          </select>
          {current?.is_k3s && <span className="k3s-badge">K3S</span>}
        </>
      )}

      <div className="sep" />

      {status.connected ? (
        <span className="conn">
          <span className="dot running" />
          Connected
        </span>
      ) : connecting ? (
        <span className="conn">
          <span className="dot pending" />
          Connecting…
        </span>
      ) : (
        <span className="conn">
          <span className="dot failed" />
          Disconnected
          {status.current_context && (
            <button className="btn sm" style={{ marginLeft: 6 }} onClick={props.onReconnect}>
              Reconnect
            </button>
          )}
        </span>
      )}

      {status.cluster_version && status.connected && (
        <span className="mono ctx-label">{status.cluster_version}</span>
      )}

      <div className="spacer" />

      <button className="btn sm" title="Open a terminal scoped to the active cluster" onClick={props.onOpenTerminal}>
        ⌨ Terminal
      </button>
      <span className="ctx-label">Auto: {props.autoRefreshSecs}s</span>
      <button className="btn sm" onClick={props.onRefresh}>
        Refresh
      </button>
    </header>
  );
}
