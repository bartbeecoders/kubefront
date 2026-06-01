import type { Selection, TableData } from "../types";
import { statusClass } from "../views";

const MONO_COLS = new Set(["Age", "Cluster IP", "Capacity", "Last Schedule"]);

interface Props {
  kind: string;
  data: TableData;
  empty: string;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
}

/** Renders a generic headers + rows projection; rows are selectable. */
export function ResourceTable({ kind, data, empty, selected, onSelect }: Props) {
  if (!data.rows.length) {
    return <div className="empty">{empty}</div>;
  }
  const nsIdx = data.headers.indexOf("Namespace");

  return (
    <div className="table-wrap">
      <table className="kt">
        <thead>
          <tr>
            {data.headers.map((h, i) => (
              <th key={i} className={i === 0 ? "name" : undefined}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {data.rows.map((row, ri) => {
            const name = row[0];
            const namespace = nsIdx >= 0 ? row[nsIdx] : null;
            const isSel =
              !!selected &&
              selected.kind === kind &&
              selected.name === name &&
              (selected.namespace ?? null) === (namespace ?? null);
            return (
              <tr
                key={ri}
                className={`clickable${isSel ? " selected" : ""}`}
                onClick={() =>
                  onSelect({
                    kind,
                    name,
                    namespace,
                    summary: data.headers
                      .map((h, i): [string, string] => [h, row[i]])
                      .filter(([h]) => h !== "Name"),
                  })
                }
              >
                {row.map((cell, ci) => (
                  <Cell key={ci} header={data.headers[ci]} value={cell} first={ci === 0} />
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Cell({ header, value, first }: { header: string; value: string; first: boolean }) {
  if (header === "Status") {
    const cls = statusClass(value);
    return <td>{cls ? <span className={`pill ${cls}`}>{value}</span> : value}</td>;
  }
  if (first) return <td className="name">{value}</td>;
  if (MONO_COLS.has(header)) return <td className="mono">{value}</td>;
  return <td>{value}</td>;
}
