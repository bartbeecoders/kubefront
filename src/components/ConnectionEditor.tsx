import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { KubeconfigEntry } from "../types";

interface Props {
  /** The connection being edited (seeds the form). */
  entry: KubeconfigEntry;
  onClose: () => void;
  /** Called after a successful save so the caller can refresh settings. */
  onSaved: () => void;
}

/** Modal editor for an existing connection. Common fields (name, description,
 *  namespace) apply to both kinds; endpoint / CA / insecure are Remote-only. The
 *  Direct file path is shown read-only — pick a different file via "Add kubeconfig". */
export function ConnectionEditor({ entry, onClose, onSaved }: Props) {
  const isRemote = entry.mode === "Remote";

  const [name, setName] = useState(entry.name);
  const [description, setDescription] = useState(entry.description ?? "");
  const [namespace, setNamespace] = useState(entry.namespace ?? "");
  const [endpoint, setEndpoint] = useState(entry.endpoint ?? "");
  const [caPath, setCaPath] = useState(entry.ca_path ?? "");
  const [insecure, setInsecure] = useState(entry.insecure);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  async function browseCa() {
    const res = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Certificate", extensions: ["pem", "crt", "cer", "ca"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof res === "string") setCaPath(res);
  }

  async function testRemote() {
    setTesting(true);
    setTestResult(null);
    try {
      const st = await api.testRemoteConnection(endpoint.trim(), caPath.trim() || null, insecure);
      setTestResult(
        st.connected
          ? `✓ Reachable${st.cluster_version ? ` — ${st.cluster_version}` : ""}`
          : `✗ ${st.error ?? "unreachable"}`,
      );
    } catch (e) {
      setTestResult(`✗ ${String(e)}`);
    } finally {
      setTesting(false);
    }
  }

  async function onSave() {
    if (!name.trim()) {
      setError("Name cannot be empty.");
      return;
    }
    if (isRemote && !endpoint.trim()) {
      setError("Endpoint cannot be empty.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.updateConnection(
        entry.id,
        name.trim(),
        description.trim() || null,
        namespace.trim() || null,
        isRemote ? endpoint.trim() : null,
        isRemote ? caPath.trim() || null : null,
        isRemote ? insecure : false,
      );
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()} style={{ minWidth: 460 }}>
        <div className="modal-title">Edit {isRemote ? "Remote" : "Direct"} Connection</div>

        <div className="field">
          <label>Name</label>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} />
        </div>

        <div className="field">
          <label>Description</label>
          <input
            className="input"
            placeholder="optional…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>

        <div className="field">
          <label>Namespace</label>
          <input
            className="input"
            style={{ width: 200 }}
            placeholder="default"
            title='Namespace scope while this connection is active. Empty = use the global default; "All" = every namespace.'
            value={namespace}
            onChange={(e) => setNamespace(e.target.value)}
          />
        </div>

        {isRemote ? (
          <>
            <div className="field">
              <label>Endpoint</label>
              <input
                className="input"
                placeholder="https://server/site/connection"
                value={endpoint}
                onChange={(e) => {
                  setEndpoint(e.target.value);
                  setTestResult(null);
                }}
              />
              <div className="desc">Backend base URL. The app appends /api/… itself.</div>
            </div>
            <div className="field">
              <label>CA certificate path</label>
              <div className="row">
                <input
                  className="input"
                  style={{ flex: 1 }}
                  placeholder="optional — for an internal/self-signed proxy CA"
                  value={caPath}
                  onChange={(e) => setCaPath(e.target.value)}
                />
                <button className="btn" onClick={browseCa}>
                  Browse…
                </button>
              </div>
            </div>
            <label className="row" style={{ gap: 6, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={insecure}
                onChange={(e) => setInsecure(e.target.checked)}
              />
              Insecure (skip TLS verification)
            </label>
            <div className="row" style={{ gap: 8, marginTop: 8, alignItems: "center" }}>
              <button className="btn" disabled={!endpoint.trim() || testing} onClick={testRemote}>
                {testing ? "Testing…" : "Test"}
              </button>
              {testResult && <span className="desc">{testResult}</span>}
            </div>
          </>
        ) : (
          <div className="field">
            <label>Kubeconfig path</label>
            <input className="input mono" value={entry.path} readOnly disabled />
            <div className="desc">
              The file path is fixed. To use a different file, add it via “Add kubeconfig file…”.
            </div>
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
