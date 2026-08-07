import { useMemo, useState } from "react";
import type { Selection, TableData } from "../types";
import { DELETABLE_KINDS, RESTARTABLE_KINDS, statusClass } from "../views";
import { ariaSort, rowMatches, sortMark, sortRows, useTableSort } from "../tablesort";
import { CopyButton } from "./CopyButton";

const MONO_COLS = new Set(["Age", "Cluster IP", "Capacity", "Last Schedule"]);

interface Props {
  kind: string;
  data: TableData;
  empty: string;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
  onDelete?: (kind: string, namespace: string | null, name: string) => void;
  onRestart?: (kind: string, namespace: string | null, name: string) => void;
}

/** Renders a generic headers + rows projection; rows are selectable,
 *  columns sort on header click, and a filter box matches any cell. */
export function ResourceTable({ kind, data, empty, selected, onSelect, onDelete, onRestart }: Props) {
  const { sort, toggle } = useTableSort();
  const [filter, setFilter] = useState("");

  const visible = useMemo(
    () =>
      sortRows(
        data.rows.filter((row) => rowMatches(row, filter)),
        sort,
        (row, col) => row[col] ?? "",
      ),
    [data.rows, filter, sort],
  );

  if (!data.rows.length) {
    return <div className="empty">{empty}</div>;
  }
  const nsIdx = data.headers.indexOf("Namespace");
  const canDelete = !!onDelete && DELETABLE_KINDS.has(kind);
  const canRestart = !!onRestart && RESTARTABLE_KINDS.has(kind);
  const hasActions = canDelete || canRestart;

  return (
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
            ({visible.length}/{data.rows.length})
          </span>
        )}
      </div>
      <div className="table-wrap">
        <table className="kt">
          <thead>
            <tr>
              {data.headers.map((h, i) => (
                <th
                  key={i}
                  className={`sortable${i === 0 ? " name" : ""}`}
                  aria-sort={ariaSort(sort, i)}
                  onClick={() => toggle(i)}
                >
                  {h}
                  <span className="sort-mark">{sortMark(sort, i)}</span>
                </th>
              ))}
              {hasActions && <th className="actions" />}
            </tr>
          </thead>
          <tbody>
            {visible.length === 0 && (
              <tr>
                <td className="empty" colSpan={data.headers.length + (hasActions ? 1 : 0)}>
                  No rows match "{filter}".
                </td>
              </tr>
            )}
            {visible.map((row) => {
              const name = row[0];
              const namespace = nsIdx >= 0 ? row[nsIdx] : null;
              const isSel =
                !!selected &&
                selected.kind === kind &&
                selected.name === name &&
                (selected.namespace ?? null) === (namespace ?? null);
              return (
                <tr
                  key={namespace ? `${namespace}/${name}` : name}
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
                  {hasActions && (
                    <td className="actions">
                      <span className="row-actions">
                        {canRestart && (
                          <button
                            className="btn ghost sm"
                            title="Rolling restart"
                            onClick={(e) => {
                              e.stopPropagation();
                              onRestart!(kind, namespace, name);
                            }}
                          >
                            ↻
                          </button>
                        )}
                        {canDelete && (
                          <button
                            className="btn ghost sm danger-hover"
                            title="Delete"
                            onClick={(e) => {
                              e.stopPropagation();
                              onDelete!(kind, namespace, name);
                            }}
                          >
                            🗑
                          </button>
                        )}
                      </span>
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function Cell({ header, value, first }: { header: string; value: string; first: boolean }) {
  if (header === "Status") {
    const cls = statusClass(value);
    return <td>{cls ? <span className={`pill ${cls}`}>{value}</span> : value}</td>;
  }
  if (first)
    return (
      <td className="name">
        <span className="name-cell">
          {value}
          <CopyButton text={value} title="Copy name" />
        </span>
      </td>
    );
  if (MONO_COLS.has(header)) return <td className="mono">{value}</td>;
  return <td>{value}</td>;
}
