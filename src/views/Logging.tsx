import type { PodRow } from "../types";

interface Props {
  pods: PodRow[];
  onOpenLogs: (pod: PodRow) => void;
}

export function LoggingView({ pods, onOpenLogs }: Props) {
  return (
    <div>
      <div className="page-head">
        <h1>Logging</h1>
        <span className="count">({pods.length} pods)</span>
      </div>
      {pods.length === 0 ? (
        <div className="empty">
          No pods available. Connect to a cluster and visit the Pods page.
        </div>
      ) : (
        <>
          <div className="hint" style={{ marginTop: 0, marginBottom: 12 }}>
            Open a live, streaming log window for any pod. Multiple windows are supported.
          </div>
          <div className="table-wrap">
            <table className="kt">
              <thead>
                <tr>
                  <th className="name">Pod</th>
                  <th>Namespace</th>
                  <th>Containers</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {pods.map((p) => (
                  <tr key={`${p.namespace}/${p.name}`}>
                    <td className="name">{p.name}</td>
                    <td>{p.namespace}</td>
                    <td>{p.containers.length}</td>
                    <td>
                      <button className="btn sm primary" onClick={() => onOpenLogs(p)}>
                        📜 Stream logs
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
