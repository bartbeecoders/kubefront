import type { KubeStatus } from "../types";

interface Props {
  status: KubeStatus;
  onConnect: (ctx: string) => void;
}

export function ClustersView({ status, onConnect }: Props) {
  const { contexts, current_context } = status;
  return (
    <div>
      <div className="page-head">
        <h1>Clusters</h1>
        <span className="count">({contexts.length} contexts)</span>
      </div>
      {contexts.length === 0 ? (
        <div className="empty">No contexts found in the loaded kubeconfig.</div>
      ) : (
        <div className="table-wrap">
          <table className="kt">
            <thead>
              <tr>
                <th className="name">Context</th>
                <th>Cluster</th>
                <th>Server</th>
                <th>K3S</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {contexts.map((c) => {
                const isCurrent = current_context === c.name;
                return (
                  <tr key={c.name} className={isCurrent ? "selected" : undefined}>
                    <td className="name">
                      {isCurrent ? <span style={{ color: "var(--status-running)" }}>● </span> : ""}
                      {c.name}
                    </td>
                    <td>{c.cluster}</td>
                    <td className="mono">{c.server}</td>
                    <td>{c.is_k3s ? <span className="k3s-badge">K3S</span> : "—"}</td>
                    <td>
                      {isCurrent ? (
                        <button className="btn sm" disabled>
                          Active
                        </button>
                      ) : (
                        <button className="btn sm primary" onClick={() => onConnect(c.name)}>
                          Connect
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <div className="hint">
        Contexts from the loaded kubeconfig • click Connect to switch clusters
      </div>
    </div>
  );
}
