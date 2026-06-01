import type { NodeRow, PodRow, TableData } from "../types";

interface Props {
  connected: boolean;
  pods: PodRow[];
  nodes: NodeRow[];
  tables: Record<string, TableData>;
}

export function MonitoringView({ connected, pods, nodes, tables }: Props) {
  if (!connected) {
    return (
      <div>
        <div className="page-head">
          <h1>Monitoring</h1>
        </div>
        <div className="empty">Connect to a cluster to see live metrics.</div>
      </div>
    );
  }

  let running = 0,
    pending = 0,
    failed = 0,
    other = 0;
  for (const p of pods) {
    switch (p.phase) {
      case "Running":
        running++;
        break;
      case "Pending":
      case "ContainerCreating":
        pending++;
        break;
      case "Failed":
        failed++;
        break;
      default:
        other++;
    }
  }
  const nodesReady = nodes.filter((n) => n.status === "Ready").length;
  const nsCount = tables["namespaces"]?.rows.length ?? 0;
  const deployCount = tables["deployments"]?.rows.length ?? 0;
  const svcCount = tables["services"]?.rows.length ?? 0;

  const stats = [
    { l: "Nodes ready", v: `${nodesReady}/${nodes.length}` },
    { l: "Namespaces", v: nsCount },
    { l: "Pods (total)", v: pods.length },
    { l: "Deployments", v: deployCount },
    { l: "Services", v: svcCount },
  ];

  return (
    <div>
      <div className="page-head">
        <h1>Monitoring</h1>
        <span className="count">cluster overview</span>
      </div>

      <div className="stat-grid">
        {stats.map((s) => (
          <div className="stat-card" key={s.l}>
            <div className="v">{s.v}</div>
            <div className="l">{s.l}</div>
          </div>
        ))}
      </div>

      <div className="section-title">Pod health</div>
      <div className="health-row">
        <span className="h" style={{ color: "var(--status-running)" }}>
          ● {running} Running
        </span>
        <span className="h" style={{ color: "var(--status-pending)" }}>
          ● {pending} Pending
        </span>
        <span className="h" style={{ color: "var(--status-failed)" }}>
          ● {failed} Failed
        </span>
        <span className="h" style={{ color: "var(--status-succeeded)" }}>
          ● {other} Other
        </span>
      </div>

      <div className="hint">
        Lightweight built-in overview. For full metrics, integrate metrics-server / Prometheus.
      </div>
    </div>
  );
}
