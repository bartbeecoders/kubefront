import type { AppState, ColorSchemeInfo, ThemeMode } from "../types";

interface Props {
  settings: AppState;
  schemes: ColorSchemeInfo[];
  onChange: (patch: Partial<AppState>) => void;
  onApply: () => void;
  onReset: () => void;
  onBrowse: () => void;
  onAddKubeconfig: () => void;
  onSwitch: (path: string) => void;
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

  const currentAccentHex = settings.custom_accent
    ? rgbToHex(settings.custom_accent)
    : schemes.find((s) => s.key === settings.color_scheme)?.hex ?? "#326ce5";

  function updateEntry(id: string, patch: { name?: string; description?: string | null }) {
    onChange({
      kubeconfigs: settings.kubeconfigs.map((e) => (e.id === id ? { ...e, ...patch } : e)),
    });
  }

  return (
    <div className="settings">
      <div className="page-head">
        <h1>Settings</h1>
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
        <div className="desc">Scopes resource lists. "All" lists every namespace.</div>
      </div>

      {/* Kubeconfig management */}
      <div className="section-title">Kubeconfig Management</div>
      <div className="desc" style={{ marginBottom: 10 }}>
        Give each cluster a friendly name and optional description. Saved to settings.json.
      </div>
      <button className="btn" onClick={props.onAddKubeconfig}>
        ＋ Add kubeconfig file…
      </button>

      <div style={{ marginTop: 12 }}>
        {settings.kubeconfigs.length === 0 ? (
          <div className="empty">No kubeconfigs registered yet.</div>
        ) : (
          <div className="table-wrap">
            <table className="kt">
              <thead>
                <tr>
                  <th className="name">Name</th>
                  <th>Description</th>
                  <th>Path</th>
                  <th></th>
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
                      <td className="mono" title={e.path}>
                        {e.path.length > 30 ? "…" + e.path.slice(-29) : e.path}
                      </td>
                      <td>
                        <div className="row">
                          {active ? (
                            <button className="btn sm" disabled>
                              Active
                            </button>
                          ) : (
                            <button className="btn sm" onClick={() => props.onSwitch(e.path)}>
                              Switch
                            </button>
                          )}
                          <button className="btn sm ghost" onClick={() => props.onRemove(e.id)}>
                            ✕
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
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
          {["debug", "info", "warn", "error"].map((l) => (
            <option key={l} value={l}>
              {l.toUpperCase()}
            </option>
          ))}
        </select>
        <div className="desc">Takes effect after restart (respects RUST_LOG if set).</div>
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
