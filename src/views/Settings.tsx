import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { AppState, ColorSchemeInfo, ThemeMode } from "../types";

interface Props {
  settings: AppState;
  schemes: ColorSchemeInfo[];
  onChange: (patch: Partial<AppState>) => void;
  onApply: () => void;
  onReset: () => void;
  onBrowse: () => void;
  onAddKubeconfig: () => void;
  /** Open the Azure AKS import wizard. */
  onAddAks: () => void;
  /** Make a connection (Direct or Remote) active and connect to it. */
  onSelect: (id: string) => void;
  /** Register a remote backend connection. */
  onAddRemote: (name: string, endpoint: string, caPath: string | null, insecure: boolean) => void;
  /** Open the editor for an existing connection. */
  onEdit: (id: string) => void;
  onRemove: (id: string) => void;
  onClose: () => void;
}

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ];
}
function rgbToHex(rgb: [number, number, number]): string {
  return "#" + rgb.map((c) => c.toString(16).padStart(2, "0")).join("");
}

export function SettingsView(props: Props) {
  const { settings, schemes, onChange } = props;

  const [logPath, setLogPath] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    api.logPath().then(setLogPath).catch(() => {});
    getVersion().then(setVersion).catch(() => {});
  }, []);

  // Remote-connection add form.
  const [rName, setRName] = useState("");
  const [rEndpoint, setREndpoint] = useState("");
  const [rCa, setRCa] = useState("");
  const [rInsecure, setRInsecure] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);

  async function browseCa() {
    const res = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Certificate", extensions: ["pem", "crt", "cer", "ca"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof res === "string") setRCa(res);
  }

  async function testRemote() {
    setTesting(true);
    setTestResult(null);
    try {
      const st = await api.testRemoteConnection(rEndpoint.trim(), rCa.trim() || null, rInsecure);
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

  function submitRemote() {
    props.onAddRemote(rName.trim(), rEndpoint.trim(), rCa.trim() || null, rInsecure);
    setRName("");
    setREndpoint("");
    setRCa("");
    setRInsecure(false);
    setTestResult(null);
  }

  const currentAccentHex = settings.custom_accent
    ? rgbToHex(settings.custom_accent)
    : schemes.find((s) => s.key === settings.color_scheme)?.hex ?? "#326ce5";

  function updateEntry(
    id: string,
    patch: { name?: string; description?: string | null; namespace?: string | null },
  ) {
    onChange({
      kubeconfigs: settings.kubeconfigs.map((e) => (e.id === id ? { ...e, ...patch } : e)),
    });
  }

  return (
    <div className="settings">
      <div className="page-head">
        <h1>Settings</h1>
        {version && <span className="version-badge">v{version}</span>}
      </div>

      {/* Kubeconfig path */}
      <div className="field">
        <label>Kubeconfig file path</label>
        <div className="row">
          <input
            className="input"
            style={{ flex: 1 }}
            placeholder="Leave empty to use last-used or quick buttons"
            value={settings.kubeconfig_path ?? ""}
            onChange={(e) =>
              onChange({ kubeconfig_path: e.target.value.trim() || null })
            }
          />
          <button className="btn" onClick={props.onBrowse}>
            Browse…
          </button>
        </div>
        <div className="desc">Used on next Apply / reconnect and at startup.</div>
      </div>

      {/* Default namespace */}
      <div className="field">
        <label>Default namespace</label>
        <input
          className="input"
          style={{ width: 240 }}
          value={settings.default_namespace}
          onChange={(e) => onChange({ default_namespace: e.target.value.trim() || "All" })}
        />
        <div className="desc">
          Fallback scope for resource lists. "All" lists every namespace. A per-connection
          namespace (below) overrides this while that kubeconfig is active.
        </div>
      </div>

      {/* Kubeconfig management */}
      <div className="section-title">Kubeconfig Management</div>
      <div className="desc" style={{ marginBottom: 10 }}>
        Give each cluster a friendly name, optional description, and its own namespace scope
        (empty = use the default namespace above; auto-filled from the kubeconfig's context on
        first connect). Saved to settings.json.
      </div>
      <div className="row" style={{ gap: 8 }}>
        <button className="btn" onClick={props.onAddKubeconfig}>
          ＋ Add kubeconfig file…
        </button>
        <button className="btn" onClick={props.onAddAks}>
          ＋ Add AKS cluster…
        </button>
      </div>

      <div style={{ marginTop: 12 }}>
        {settings.kubeconfigs.length === 0 ? (
          <div className="empty">No kubeconfigs registered yet.</div>
        ) : (
          <div className="table-wrap">
            <table className="kt">
              <thead>
                <tr>
                  <th className="name">Name</th>
                  <th>Type</th>
                  <th>Description</th>
                  <th>Namespace</th>
                  <th>Path / Endpoint</th>
                  <th className="actions"></th>
                </tr>
              </thead>
              <tbody>
                {settings.kubeconfigs.map((e) => {
                  const active = settings.active_kubeconfig_id === e.id;
                  return (
                    <tr key={e.id} className={active ? "selected" : undefined}>
                      <td>
                        <input
                          className="input"
                          style={{ width: 150 }}
                          value={e.name}
                          onChange={(ev) => updateEntry(e.id, { name: ev.target.value })}
                        />
                      </td>
                      <td>
                        <span className={`pill ${e.mode === "Remote" ? "pill-remote" : "pill-direct"}`}>
                          {e.mode === "Remote" ? "Remote" : "Direct"}
                        </span>
                      </td>
                      <td>
                        <input
                          className="input"
                          style={{ width: "100%" }}
                          placeholder="optional…"
                          value={e.description ?? ""}
                          onChange={(ev) =>
                            updateEntry(e.id, { description: ev.target.value || null })
                          }
                        />
                      </td>
                      <td>
                        <input
                          className="input"
                          style={{ width: 110 }}
                          placeholder="default"
                          title='Namespace scope while this kubeconfig is active. Empty = use the global default; "All" = every namespace.'
                          value={e.namespace ?? ""}
                          onChange={(ev) =>
                            updateEntry(e.id, { namespace: ev.target.value.trim() || null })
                          }
                        />
                      </td>
                      <td className="mono" title={e.path}>
                        {e.path.length > 30 ? "…" + e.path.slice(-29) : e.path}
                      </td>
                      <td className="actions">
                        <span className="row-actions">
                          {active ? (
                            <button className="btn sm" disabled>
                              Active
                            </button>
                          ) : (
                            <button className="btn sm" onClick={() => props.onSelect(e.id)}>
                              {e.mode === "Remote" ? "Connect" : "Switch"}
                            </button>
                          )}
                          <button className="btn sm" onClick={() => props.onEdit(e.id)}>
                            Edit
                          </button>
                          <button className="btn sm ghost" onClick={() => props.onRemove(e.id)}>
                            ✕
                          </button>
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Remote connections */}
      <div className="section-title">Remote Connections (via backend)</div>
      <div className="desc" style={{ marginBottom: 10 }}>
        Connect to clusters reachable only through a reverse proxy (port 443) by adding a
        kubefront-backend endpoint, e.g.{" "}
        <span className="mono">https://server/k3s-server1/connection1</span>. Added connections
        appear in the list above (type "Remote") and on the Dashboard.
      </div>
      <div className="field">
        <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
          <input
            className="input"
            placeholder="Name (e.g. Site 1 Prod)"
            style={{ width: 180 }}
            value={rName}
            onChange={(e) => setRName(e.target.value)}
          />
          <input
            className="input"
            placeholder="https://server/site/connection"
            style={{ flex: 1, minWidth: 260 }}
            value={rEndpoint}
            onChange={(e) => setREndpoint(e.target.value)}
          />
        </div>
        <div className="row" style={{ gap: 8, marginTop: 8, flexWrap: "wrap" }}>
          <input
            className="input"
            placeholder="CA certificate path (optional)"
            style={{ flex: 1, minWidth: 220 }}
            value={rCa}
            onChange={(e) => setRCa(e.target.value)}
          />
          <button className="btn" onClick={browseCa}>
            Browse…
          </button>
          <label
            className="row"
            style={{ gap: 4, alignItems: "center" }}
            title="Skip TLS verification — only for trusted networks with a self-signed proxy"
          >
            <input
              type="checkbox"
              checked={rInsecure}
              onChange={(e) => setRInsecure(e.target.checked)}
            />
            Insecure
          </label>
        </div>
        <div className="row" style={{ gap: 8, marginTop: 8, alignItems: "center" }}>
          <button className="btn" disabled={!rEndpoint.trim() || testing} onClick={testRemote}>
            {testing ? "Testing…" : "Test"}
          </button>
          <button
            className="btn primary"
            disabled={!rName.trim() || !rEndpoint.trim()}
            onClick={submitRemote}
          >
            ＋ Add remote
          </button>
          {testResult && <span className="desc">{testResult}</span>}
        </div>
      </div>

      {/* Theme */}
      <div className="section-title">Appearance</div>
      <div className="field">
        <label>Color theme</label>
        <select
          className="select"
          value={settings.theme_mode}
          onChange={(e) => onChange({ theme_mode: e.target.value as ThemeMode })}
        >
          <option value="Dark">Dark (default)</option>
          <option value="Light">Light</option>
          <option value="Custom">Custom accent</option>
        </select>
      </div>

      {settings.theme_mode === "Custom" && (
        <div className="field">
          <label>Color scheme presets</label>
          <div className="swatches">
            {schemes.map((s) => (
              <div
                key={s.key}
                className={`swatch${settings.color_scheme === s.key ? " active" : ""}`}
                style={{ background: s.hex }}
                title={s.label}
                onClick={() =>
                  onChange({ color_scheme: s.key, custom_accent: hexToRgb(s.hex) })
                }
              />
            ))}
          </div>
          <div className="row" style={{ marginTop: 10 }}>
            <span className="ctx-label">Fine tune:</span>
            <input
              type="color"
              value={currentAccentHex}
              onChange={(e) => onChange({ custom_accent: hexToRgb(e.target.value) })}
            />
          </div>
        </div>
      )}

      <div className="field">
        <label>Font scale ({settings.font_scale.toFixed(2)}×)</label>
        <input
          type="range"
          min={0.75}
          max={1.6}
          step={0.05}
          value={settings.font_scale}
          onChange={(e) => onChange({ font_scale: parseFloat(e.target.value) })}
        />
        <div className="desc">Affects all text. 1.0 = normal. Changes apply instantly.</div>
      </div>

      <div className="field">
        <label>Auto-refresh interval (seconds)</label>
        <input
          className="input"
          type="number"
          min={1}
          max={120}
          style={{ width: 120 }}
          value={settings.auto_refresh_secs}
          onChange={(e) =>
            onChange({ auto_refresh_secs: Math.max(1, parseInt(e.target.value) || 5) })
          }
        />
      </div>

      <div className="field">
        <label>Log level</label>
        <select
          className="select"
          value={settings.log_level}
          onChange={(e) => onChange({ log_level: e.target.value })}
        >
          {["trace", "debug", "info", "warn", "error"].map((l) => (
            <option key={l} value={l}>
              {l.toUpperCase()}
            </option>
          ))}
        </select>
        <div className="desc">Takes effect after restart (respects RUST_LOG if set).</div>
        {logPath && (
          <div className="desc">
            Log file:{" "}
            <span className="mono" style={{ userSelect: "text" }}>
              {logPath}
            </span>
          </div>
        )}
      </div>

      <div className="detail-sep" />
      <div className="row">
        <button className="btn primary" onClick={props.onApply}>
          Apply &amp; Reconnect
        </button>
        <button className="btn" onClick={props.onReset}>
          Reset to defaults
        </button>
        <button className="btn ghost" onClick={props.onClose}>
          Close
        </button>
      </div>
      <div className="hint">
        Theme &amp; font changes are live. Kubeconfig / namespace changes require Apply.
      </div>
    </div>
  );
}
