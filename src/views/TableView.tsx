import type { Selection, TableData } from "../types";
import type { TableView as TableViewDef } from "../views";
import { ResourceTable } from "../components/ResourceTable";

interface Props {
  def: TableViewDef;
  tables: Record<string, TableData>;
  selected: Selection | null;
  onSelect: (sel: Selection) => void;
  onDelete: (kind: string, namespace: string | null, name: string) => void;
  onRestart: (kind: string, namespace: string | null, name: string) => void;
}

/** Renders one or more selectable resource tables for a declarative table view. */
export function TableView({ def, tables, selected, onSelect, onDelete, onRestart }: Props) {
  return (
    <div>
      {def.sections.map((s) => {
        const data = tables[s.kind] ?? { headers: [], rows: [] };
        return (
          <section key={s.kind} style={{ marginBottom: 24 }}>
            <div className="page-head">
              <h1>{s.title}</h1>
              <span className="count">({data.rows.length})</span>
            </div>
            <ResourceTable
              kind={s.kind}
              data={data}
              empty={s.empty}
              selected={selected}
              onSelect={onSelect}
              onDelete={onDelete}
              onRestart={onRestart}
            />
          </section>
        );
      })}
    </div>
  );
}
