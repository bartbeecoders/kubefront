import { useMemo } from "react";
import type { PodRow, Selection } from "../types";
import { statusClass } from "../views";

interface Props {
  pods: PodRow[];
  filter: string;
  setFilter: (s: string) => void;
  nsFilter: string;
  setNsFilter: (s: string) => void;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
  onOpenLogs: (pod: PodRow) => void;
}

export const podKey = (p: PodRow) => `${p.namespace}/${p.name}`;

export function podSelection(p: PodRow): Selection {
  return {
    kind: "pods",
    name: p.name,
    namespace: p.namespace,
    summary: [
      ["Namespace", p.namespace],
      ["Status", p.phase],
      ["Ready", p.ready],
      ["Restarts", String(p.restarts)],
      ["Age", p.age],
      ["Node", p.node],
    ],
  };
}

export function PodsView({
  pods,
  filter,
  setFilter,
  nsFilter,
  setNsFilter,
  selected,
  onSelect,
  onOpenLogs,
}: Props) {
  const namespaces = useMemo(() => {
    const set = new Set(pods.map((p) => p.namespace));
    return ["All", ...Array.from(set).sort()];
  }, [pods]);

  const lower = filter.toLowerCase();
  const filtered = pods.filter((p) => {
    if (lower && !p.name.toLowerCase().includes(lower) && !p.namespace.toLowerCase().includes(lower))
      return false;
    if (nsFilter !== "All" && p.namespace !== nsFilter) return false;
    return true;
  });

  return (
    <div>
      <div className="toolbar">
        <input
          className="input"
          placeholder="Filter pods…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{ width: 220 }}
        />
        <span className="ctx-label">Namespace</span>
        <select className="select" value={nsFilter} onChange={(e) => setNsFilter(e.target.value)}>
          {namespaces.map((ns) => (
            <option key={ns} value={ns}>
              {ns}
            </option>
          ))}
        </select>
        <button
          className="btn sm"
          onClick={() => {
            setFilter("");
            setNsFilter("All");
          }}
        >
          Clear filters
        </button>
        <span className="count">({filtered.length} pods)</span>
      </div>

      <div className="table-wrap">
        <table className="kt">
          <thead>
            <tr>
              <th className="name">Name</th>
              <th>Namespace</th>
              <th>Status</th>
              <th>Ready</th>
              <th>Restarts</th>
              <th>Age</th>
              <th>Node</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((p) => {
              const key = podKey(p);
              const isSel =
                !!selected &&
                selected.kind === "pods" &&
                selected.name === p.name &&
                selected.namespace === p.namespace;
              return (
                <tr
                  key={key}
                  className={`clickable${isSel ? " selected" : ""}`}
                  onClick={() => onSelect(podSelection(p))}
                >
                  <td className="name">{p.name}</td>
                  <td>{p.namespace}</td>
                  <td>
                    <span className={`pill ${statusClass(p.phase)}`}>{p.phase}</span>
                  </td>
                  <td className="mono">{p.ready}</td>
                  <td>{p.restarts}</td>
                  <td className="mono">{p.age}</td>
                  <td>{p.node}</td>
                  <td>
                    <button
                      className="btn sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onOpenLogs(p);
                      }}
                    >
                      📜 Logs
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className="hint">Live pods from cluster • auto-refresh • click a row for details</div>
    </div>
  );
}
