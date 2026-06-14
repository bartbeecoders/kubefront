import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { ClusterType, Environment, KubeconfigEntry } from "../types";

const CLUSTER_TYPES: ClusterType[] = ["K3s", "K8s", "Aks"];
const ENVIRONMENTS: Environment[] = ["Dev", "Val", "Prod"];

/** Parse a coordinate text field → number | null (empty/invalid = null). */
function parseCoord(s: string): number | null {
  const t = s.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

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

  // World-view / inventory metadata.
  const [city, setCity] = useState(entry.city ?? "");
  const [country, setCountry] = useState(entry.country ?? "");
  const [latitude, setLatitude] = useState(entry.latitude != null ? String(entry.latitude) : "");
  const [longitude, setLongitude] = useState(entry.longitude != null ? String(entry.longitude) : "");
  const [clusterType, setClusterType] = useState<ClusterType | "">(entry.cluster_type ?? "");
  const [plant, setPlant] = useState(entry.plant ?? "");
  const [environment, setEnvironment] = useState<Environment | "">(entry.environment ?? "");

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
      await api.updateConnection(entry.id, {
        name: name.trim(),
        description: description.trim() || null,
        namespace: namespace.trim() || null,
        endpoint: isRemote ? endpoint.trim() : null,
        ca_path: isRemote ? caPath.trim() || null : null,
        insecure: isRemote ? insecure : false,
        city: city.trim() || null,
        country: country.trim() || null,
        latitude: parseCoord(latitude),
        longitude: parseCoord(longitude),
        cluster_type: clusterType || null,
        plant: plant.trim() || null,
        environment: environment || null,
      });
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div className="modal conn-editor" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">
          Edit {isRemote ? "Remote" : "Direct"} Connection
          <span className="conn-editor-sub" title={entry.name}>
            {entry.name}
          </span>
        </div>

        <div className="conn-editor-body">
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

        <div className="section-title" style={{ marginTop: 14 }}>
          Location & inventory
        </div>
        <div className="desc" style={{ marginBottom: 8 }}>
          Used for the dashboard world map and filtering. The map places this cluster from the
          country (or precise latitude/longitude if given).
        </div>

        <div className="row" style={{ gap: 10 }}>
          <div className="field" style={{ flex: 1 }}>
            <label>City</label>
            <input
              className="input"
              placeholder="e.g. Visp"
              value={city}
              onChange={(e) => setCity(e.target.value)}
            />
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>Country</label>
            <input
              className="input"
              placeholder="e.g. Switzerland"
              value={country}
              onChange={(e) => setCountry(e.target.value)}
            />
          </div>
        </div>

        <div className="row" style={{ gap: 10 }}>
          <div className="field" style={{ flex: 1 }}>
            <label>Latitude</label>
            <input
              className="input"
              placeholder="optional — overrides country"
              value={latitude}
              onChange={(e) => setLatitude(e.target.value)}
            />
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>Longitude</label>
            <input
              className="input"
              placeholder="optional"
              value={longitude}
              onChange={(e) => setLongitude(e.target.value)}
            />
          </div>
        </div>

        <div className="row" style={{ gap: 10 }}>
          <div className="field" style={{ flex: 1 }}>
            <label>Cluster type</label>
            <select
              className="input"
              value={clusterType}
              onChange={(e) => setClusterType(e.target.value as ClusterType | "")}
            >
              <option value="">—</option>
              {CLUSTER_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t === "K3s" ? "K3S" : t === "K8s" ? "K8S" : "AKS"}
                </option>
              ))}
            </select>
          </div>
          <div className="field" style={{ flex: 1 }}>
            <label>Environment</label>
            <select
              className="input"
              value={environment}
              onChange={(e) => setEnvironment(e.target.value as Environment | "")}
            >
              <option value="">—</option>
              {ENVIRONMENTS.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="field">
          <label>Manufacturing plant</label>
          <input
            className="input"
            placeholder="optional…"
            value={plant}
            onChange={(e) => setPlant(e.target.value)}
          />
        </div>
        </div>

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
