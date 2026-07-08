import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";

export interface DataEditRequest {
  /** Which resource is being edited — decides the title and the save command. */
  kind: "configmaps" | "secrets";
  namespace: string;
  name: string;
  /** Current text entries, used to seed the form (plaintext for both kinds). */
  data: Record<string, string>;
  /** Secrets only: number of binary (non-UTF-8) entries preserved untouched. */
  binaryCount?: number;
}

interface Props {
  req: DataEditRequest;
  onClose: () => void;
  /** Called after a successful save so the caller can refresh views. */
  onSaved: () => void;
}

interface Entry {
  id: number;
  key: string;
  value: string;
}

/** Modal editor for a ConfigMap's or Secret's key/value data (add, edit, remove).
 *  Secret values are masked by default and can be revealed; binary Secret entries
 *  are left untouched by the backend and reported via `req.binaryCount`. */
export function DataEditor({ req, onClose, onSaved }: Props) {
  const isSecret = req.kind === "secrets";
  const seed = useMemo<Entry[]>(
    () => Object.entries(req.data).map(([key, value], i) => ({ id: i, key, value })),
    [req],
  );
  const [entries, setEntries] = useState<Entry[]>(seed);
  const [reveal, setReveal] = useState(!isSecret);
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
      if (isSecret) await api.updateSecret(req.namespace, req.name, data);
      else await api.updateConfigmap(req.namespace, req.name, data);
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  const binaryNote = isSecret && req.binaryCount ? req.binaryCount : 0;

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div className="modal cm-editor" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title-row">
          <div className="modal-title">{isSecret ? "Edit Secret" : "Edit ConfigMap"}</div>
          {isSecret && (
            <button
              className="btn sm"
              title={reveal ? "Hide values" : "Reveal values"}
              onClick={() => setReveal((r) => !r)}
            >
              {reveal ? "🙈 Hide" : "👁 Reveal"}
            </button>
          )}
        </div>
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
                className={`input mono cm-value${isSecret && !reveal ? " masked" : ""}`}
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

        {binaryNote > 0 && (
          <div className="dim" style={{ fontSize: "0.8em", marginTop: 8 }}>
            {binaryNote} binary {binaryNote === 1 ? "entry is" : "entries are"} preserved unchanged
            (not shown here).
          </div>
        )}

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
