import type { NodeRow, Selection } from "../types";
import { statusClass } from "../views";

interface Props {
  nodes: NodeRow[];
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
}

function nodeSelection(n: NodeRow): Selection {
  return {
    kind: "nodes",
    name: n.name,
    namespace: null,
    summary: [
      ["Status", n.status],
      ["Roles", n.roles],
      ["Kubelet Version", n.version],
      ["Age", n.age],
    ],
  };
}

export function NodesView({ nodes, selected, onSelect }: Props) {
  return (
    <div>
      <div className="page-head">
        <h1>Nodes</h1>
        <span className="count">({nodes.length})</span>
      </div>
      {nodes.length === 0 ? (
        <div className="empty">No nodes. Connect to a cluster.</div>
      ) : (
        <div className="table-wrap">
          <table className="kt">
            <thead>
              <tr>
                <th className="name">Name</th>
                <th>Status</th>
                <th>Roles</th>
                <th>Kubelet Version</th>
                <th>Age</th>
              </tr>
            </thead>
            <tbody>
              {nodes.map((n) => (
                <tr
                  key={n.name}
                  className={`clickable${
                    selected && selected.kind === "nodes" && selected.name === n.name
                      ? " selected"
                      : ""
                  }`}
                  onClick={() => onSelect(nodeSelection(n))}
                >
                  <td className="name">{n.name}</td>
                  <td>
                    <span className={`pill ${statusClass(n.status)}`}>{n.status}</span>
                  </td>
                  <td>{n.roles}</td>
                  <td className="mono">{n.version}</td>
                  <td className="mono">{n.age}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="hint">Live nodes from cluster • click for details</div>
    </div>
  );
}
