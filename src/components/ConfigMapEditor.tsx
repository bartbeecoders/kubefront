import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";

export interface ConfigMapEditRequest {
  namespace: string;
  name: string;
  /** Current `data` map of the ConfigMap, used to seed the form. */
  data: Record<string, string>;
}

interface Props {
  req: ConfigMapEditRequest;
  onClose: () => void;
  /** Called after a successful save so the caller can refresh views. */
  onSaved: () => void;
}

interface Entry {
  id: number;
  key: string;
  value: string;
}

/** Modal editor for a ConfigMap's `data` key/value pairs (add, edit, remove). */
export function ConfigMapEditor({ req, onClose, onSaved }: Props) {
  const seed = useMemo<Entry[]>(
    () => Object.entries(req.data).map(([key, value], i) => ({ id: i, key, value })),
    [req],
  );
  const [entries, setEntries] = useState<Entry[]>(seed);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const nextId = useRef(seed.length);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  function patch(id: number, change: Partial<Entry>) {
    setEntries((es) => es.map((e) => (e.id === id ? { ...e, ...change } : e)));
  }
  function remove(id: number) {
    setEntries((es) => es.filter((e) => e.id !== id));
  }
  function add() {
    setEntries((es) => [...es, { id: nextId.current++, key: "", value: "" }]);
  }

  async function onSave() {
    const keys = entries.map((e) => e.key.trim());
    if (keys.some((k) => k === "")) {
      setError("Every entry needs a key.");
      return;
    }
    const dupes = keys.filter((k, i) => keys.indexOf(k) !== i);
    if (dupes.length) {
      setError(`Duplicate key: ${[...new Set(dupes)].join(", ")}`);
      return;
    }
    const data: Record<string, string> = {};
    for (const e of entries) data[e.key.trim()] = e.value;

    setBusy(true);
    setError(null);
    try {
      await api.updateConfigmap(req.namespace, req.name, data);
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div className="modal cm-editor" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">Edit ConfigMap</div>
        <div className="dim mono" style={{ fontSize: "0.82em", marginTop: 2 }}>
          {req.namespace}/{req.name}
        </div>

        <div className="cm-entries">
          {entries.length === 0 && (
            <div className="dim" style={{ fontSize: "0.85em", padding: "8px 0" }}>
              No data entries. Add one below.
            </div>
          )}
          {entries.map((e) => (
            <div className="cm-entry" key={e.id}>
              <input
                className="input mono cm-key"
                placeholder="key"
                value={e.key}
                spellCheck={false}
                onChange={(ev) => patch(e.id, { key: ev.target.value })}
              />
              <textarea
                className="input mono cm-value"
                placeholder="value"
                value={e.value}
                spellCheck={false}
                rows={Math.min(8, Math.max(1, e.value.split("\n").length))}
                onChange={(ev) => patch(e.id, { value: ev.target.value })}
              />
              <button
                className="btn sm danger-hover cm-remove"
                title="Remove entry"
                onClick={() => remove(e.id)}
              >
                ✕
              </button>
            </div>
          ))}
        </div>

        <button className="btn sm" style={{ marginTop: 4 }} onClick={add}>
          + Add entry
        </button>

        {error && <div className="modal-error">⚠ {error}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button className="btn primary" onClick={onSave} disabled={busy}>
            {busy ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
