import { useState } from "react";
import type { NodeRow, Selection } from "../types";
import { statusClass } from "../views";
import { ariaSort, rowMatches, sortMark, sortRows, useTableSort } from "../tablesort";

interface Props {
  nodes: NodeRow[];
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
}

const NODE_COLS: { label: string; cell: (n: NodeRow) => string }[] = [
  { label: "Name", cell: (n) => n.name },
  { label: "Status", cell: (n) => n.status },
  { label: "Roles", cell: (n) => n.roles },
  { label: "Kubelet Version", cell: (n) => n.version },
  { label: "Age", cell: (n) => n.age },
];

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
  const { sort, toggle } = useTableSort();
  const [filter, setFilter] = useState("");

  const visible = sortRows(
    nodes.filter((n) => rowMatches(NODE_COLS.map((c) => c.cell(n)), filter)),
    sort,
    (n, col) => NODE_COLS[col].cell(n),
  );

  return (
    <div>
      <div className="page-head">
        <h1>Nodes</h1>
        <span className="count">({nodes.length})</span>
      </div>
      {nodes.length === 0 ? (
        <div className="empty">No nodes. Connect to a cluster.</div>
      ) : (
        <div>
          <div className="toolbar table-toolbar">
            <input
              className="input sm"
              placeholder="Filter…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            {filter && (
              <span className="count">
                ({visible.length}/{nodes.length})
              </span>
            )}
          </div>
          <div className="table-wrap">
            <table className="kt">
              <thead>
                <tr>
                  {NODE_COLS.map((c, i) => (
                    <th
                      key={c.label}
                      className={`sortable${i === 0 ? " name" : ""}`}
                      aria-sort={ariaSort(sort, i)}
                      onClick={() => toggle(i)}
                    >
                      {c.label}
                      <span className="sort-mark">{sortMark(sort, i)}</span>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {visible.length === 0 && (
                  <tr>
                    <td className="empty" colSpan={NODE_COLS.length}>
                      No rows match "{filter}".
                    </td>
                  </tr>
                )}
                {visible.map((n) => (
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
        </div>
      )}
      <div className="hint">Live nodes from cluster • click for details</div>
    </div>
  );
}
