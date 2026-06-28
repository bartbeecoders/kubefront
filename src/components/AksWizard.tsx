import { useEffect, useState } from "react";
import { api } from "../api";
import type { AksCluster, AzureStatus, AzureSubscription } from "../types";

interface Props {
  onClose: () => void;
  /** Called after a cluster is imported so the caller can refresh settings. */
  onSaved: () => void;
}

type Step = "preflight" | "subscription" | "cluster" | "confirm";

/** Wizard to browse Azure subscriptions + AKS clusters via the `az` CLI and import
 *  one as a Direct connection (AAD auth via `kubelogin`/`az login`). Mirrors the
 *  modal pattern used by ConnectionEditor. */
export function AksWizard({ onClose, onSaved }: Props) {
  const [step, setStep] = useState<Step>("preflight");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [azStatus, setAzStatus] = useState<AzureStatus | null>(null);
  const [subs, setSubs] = useState<AzureSubscription[]>([]);
  const [sub, setSub] = useState<AzureSubscription | null>(null);
  const [clusters, setClusters] = useState<AksCluster[]>([]);
  const [cluster, setCluster] = useState<AksCluster | null>(null);
  const [displayName, setDisplayName] = useState("");

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  // Preflight: probe the Azure CLI on mount.
  useEffect(() => {
    void checkAzure();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function checkAzure() {
    setBusy(true);
    setError(null);
    try {
      const st = await api.azureStatus();
      setAzStatus(st);
      if (st.installed && st.logged_in) {
        await loadSubscriptions();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function loadSubscriptions() {
    setBusy(true);
    setError(null);
    try {
      const list = await api.azureSubscriptions();
      setSubs(list);
      setStep("subscription");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function pickSubscription(s: AzureSubscription) {
    setSub(s);
    setBusy(true);
    setError(null);
    try {
      const list = await api.azureAksClusters(s.id);
      setClusters(list);
      setStep("cluster");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function pickCluster(c: AksCluster) {
    setCluster(c);
    setDisplayName(c.name);
    setError(null);
    setStep("confirm");
  }

  async function importCluster() {
    if (!sub || !cluster) return;
    setBusy(true);
    setError(null);
    try {
      await api.addAksConnection(
        sub.id,
        cluster.resource_group,
        cluster.name,
        displayName.trim() || null,
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
      <div className="modal conn-editor" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">
          Add AKS cluster
          {sub && (
            <span className="conn-editor-sub" title={sub.name}>
              {sub.name}
            </span>
          )}
        </div>

        <div className="conn-editor-body">
          {step === "preflight" && (
            <Preflight status={azStatus} busy={busy} onRecheck={checkAzure} />
          )}

          {step === "subscription" && (
            <div className="field">
              <label>Subscription</label>
              {subs.length === 0 ? (
                <div className="empty">No subscriptions found for this account.</div>
              ) : (
                <div className="table-wrap">
                  <table className="kt">
                    <thead>
                      <tr>
                        <th className="name">Name</th>
                        <th>ID</th>
                        <th className="actions"></th>
                      </tr>
                    </thead>
                    <tbody>
                      {subs.map((s) => (
                        <tr key={s.id}>
                          <td className="name">
                            {s.name}
                            {s.is_default && (
                              <span className="desc" style={{ marginLeft: 6 }}>
                                (default)
                              </span>
                            )}
                          </td>
                          <td className="mono">{s.id}</td>
                          <td className="actions">
                            <button
                              className="btn sm primary"
                              disabled={busy}
                              onClick={() => pickSubscription(s)}
                            >
                              Choose
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {step === "cluster" && (
            <div className="field">
              <label>AKS cluster</label>
              {clusters.length === 0 ? (
                <div className="empty">No AKS clusters in this subscription.</div>
              ) : (
                <div className="table-wrap">
                  <table className="kt">
                    <thead>
                      <tr>
                        <th className="name">Name</th>
                        <th>Resource group</th>
                        <th>Location</th>
                        <th>Version</th>
                        <th>State</th>
                        <th className="actions"></th>
                      </tr>
                    </thead>
                    <tbody>
                      {clusters.map((c) => (
                        <tr key={`${c.resource_group}/${c.name}`}>
                          <td className="name">
                            {c.name}
                            {c.aad_enabled && (
                              <span className="desc" style={{ marginLeft: 6 }}>
                                AAD
                              </span>
                            )}
                          </td>
                          <td>{c.resource_group}</td>
                          <td>{c.location ?? "—"}</td>
                          <td>{c.kubernetes_version ?? "—"}</td>
                          <td>{c.power_state ?? "—"}</td>
                          <td className="actions">
                            <button
                              className="btn sm primary"
                              disabled={busy}
                              onClick={() => pickCluster(c)}
                            >
                              Choose
                            </button>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {step === "confirm" && cluster && (
            <>
              <div className="field">
                <label>Connection name</label>
                <input
                  className="input"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                />
              </div>
              <div className="desc">
                KubeFront will run <span className="mono">az aks get-credentials</span> and{" "}
                <span className="mono">kubelogin convert-kubeconfig -l azurecli</span> so the
                connection authenticates with your <span className="mono">az login</span> session
                (Azure AD). <span className="mono">kubelogin</span> must be installed and on PATH.
              </div>
              <div className="field" style={{ marginTop: 10 }}>
                <label>Cluster</label>
                <div className="mono">
                  {cluster.name} · {cluster.resource_group}
                  {cluster.location ? ` · ${cluster.location}` : ""}
                </div>
              </div>
            </>
          )}
        </div>

        {error && <div className="modal-error">⚠ {error}</div>}

        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          {step === "cluster" && (
            <button className="btn" disabled={busy} onClick={() => setStep("subscription")}>
              Back
            </button>
          )}
          {step === "confirm" && (
            <>
              <button className="btn" disabled={busy} onClick={() => setStep("cluster")}>
                Back
              </button>
              <button className="btn primary" disabled={busy} onClick={importCluster}>
                {busy ? "Importing…" : "Import & add"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/** Preflight panel: shows Azure CLI status and guidance when not ready. */
function Preflight({
  status,
  busy,
  onRecheck,
}: {
  status: AzureStatus | null;
  busy: boolean;
  onRecheck: () => void;
}) {
  if (busy && !status) {
    return <div className="desc">Checking Azure CLI…</div>;
  }
  if (!status) {
    return <div className="desc">Checking Azure CLI…</div>;
  }
  if (status.installed && status.logged_in) {
    return (
      <div className="desc">
        Signed in as <span className="mono">{status.user ?? "unknown"}</span>. Loading
        subscriptions…
      </div>
    );
  }
  return (
    <div className="field">
      <div className="modal-error" style={{ marginBottom: 10 }}>
        ⚠ {status.error ?? "Azure CLI is not ready."}
      </div>
      <div className="desc">
        {status.installed
          ? "Run `az login` in a terminal to sign in, then re-check."
          : "Install the Azure CLI (https://aka.ms/azure-cli) and restart KubeFront, then re-check."}
      </div>
      <div style={{ marginTop: 10 }}>
        <button className="btn" disabled={busy} onClick={onRecheck}>
          {busy ? "Checking…" : "Re-check"}
        </button>
      </div>
    </div>
  );
}
