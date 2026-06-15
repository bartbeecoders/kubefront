import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../api";
import type { PodRow, ResourceDetail, Selection } from "../types";
import { DELETABLE_KINDS, EDITABLE_KINDS, RESTARTABLE_KINDS, statusClass } from "../views";
import { CopyButton, copyText } from "./CopyButton";
import { ManifestViewer } from "./ManifestViewer";

interface Props {
  selected: Selection | null;
  pods: PodRow[];
  /** Bumped by the parent to force a re-fetch of the detail (e.g. after an edit). */
  reloadKey?: number;
  onOpenLogs: (pod: PodRow) => void;
  onDelete: (kind: string, namespace: string | null, name: string) => void;
  onRestart: (kind: string, namespace: string | null, name: string) => void;
  onEdit: (kind: string, namespace: string | null, name: string) => void;
  onDescribe: (namespace: string, name: string) => void;
}

export function DetailPanel({
  selected,
  pods,
  reloadKey,
  onOpenLogs,
  onDelete,
  onRestart,
  onEdit,
  onDescribe,
}: Props) {
  const [detail, setDetail] = useState<ResourceDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showManifest, setShowManifest] = useState(false);

  useEffect(() => {
    if (!selected) {
      setDetail(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setDetail(null);
    setError(null);
    api
      .getResource(selected.kind, selected.namespace, selected.name)
      .then((d) => !cancelled && setDetail(d))
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
    // Re-fetch whenever the selected identity changes — or the parent bumps reloadKey.
  }, [selected?.kind, selected?.name, selected?.namespace, reloadKey]);

  if (!selected) {
    return (
      <aside className="detail">
        <h2>Details</h2>
        <div className="detail-sep" />
        <div className="dim">Select an item in the list to see its details.</div>
      </aside>
    );
  }

  const pod =
    selected.kind === "pods"
      ? pods.find((p) => p.name === selected.name && p.namespace === selected.namespace)
      : undefined;

  const canRestart = RESTARTABLE_KINDS.has(selected.kind) || selected.kind === "pods";
  const canDelete = DELETABLE_KINDS.has(selected.kind);
  const canEdit = EDITABLE_KINDS.has(selected.kind);

  return (
    <aside className="detail">
      <div className="detail-title">
        <h2 title={selected.name}>{selected.name}</h2>
        <CopyButton text={selected.name} title="Copy name" />
      </div>
      <div className="dim mono">
        {selected.kind}
        {selected.namespace ? ` · ns/${selected.namespace}` : ""}
      </div>

      {pod && (
        <div className="row" style={{ marginTop: 12 }}>
          <button className="btn primary" onClick={() => onOpenLogs(pod)}>
            📜 View Logs (live)
          </button>
          <button
            className="btn"
            title="kubectl describe pod"
            onClick={() => onDescribe(pod.namespace, pod.name)}
          >
            📋 Describe
          </button>
        </div>
      )}

      {(canRestart || canDelete || canEdit) && (
        <div className="detail-actions">
          {canEdit && (
            <button
              className="btn sm"
              title="Edit data entries"
              onClick={() => onEdit(selected.kind, selected.namespace, selected.name)}
            >
              ✎ Edit
            </button>
          )}
          {canRestart && (
            <button
              className="btn sm"
              title={
                selected.kind === "pods"
                  ? "Delete the pod; its controller recreates it"
                  : "Rolling restart"
              }
              onClick={() => onRestart(selected.kind, selected.namespace, selected.name)}
            >
              ↻ Restart
            </button>
          )}
          {canDelete && (
            <button
              className="btn sm danger"
              onClick={() => onDelete(selected.kind, selected.namespace, selected.name)}
            >
              🗑 Delete
            </button>
          )}
        </div>
      )}

      <div className="detail-sep" />

      {/* Projected summary from the table row (type-aware). */}
      {selected.summary.map(([k, v]) => (
        <Kv
          key={k}
          k={k}
          v={k === "Status" ? <span className={`pill ${statusClass(v)}`}>{v}</span> : v}
        />
      ))}

      <div className="section-title" style={{ marginTop: 18 }}>
        Metadata
      </div>
      {loading && <div className="dim">Loading…</div>}
      {error && <div className="dim" style={{ color: "var(--status-failed)" }}>{error}</div>}
      {detail && (
        <>
          <Kv k="Age" v={detail.age} />
          <MapBlock title="Labels" entries={detail.labels} />
          <MapBlock title="Annotations" entries={detail.annotations} />
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn sm" onClick={() => setShowManifest((s) => !s)}>
              {showManifest ? "Hide" : "Show"} manifest
            </button>
            <CopyButton text={() => detail.manifest} title="Copy manifest JSON" label="Copy manifest" />
          </div>
          {showManifest && <ManifestViewer text={detail.manifest} title={selected.name} />}
        </>
      )}
    </aside>
  );
}

function Kv({ k, v }: { k: string; v: ReactNode }) {
  return (
    <div className="kv">
      <span>{k}</span>
      <span>{v}</span>
    </div>
  );
}

function MapBlock({ title, entries }: { title: string; entries: [string, string][] }) {
  return (
    <div style={{ marginTop: 10 }}>
      <div className="dim" style={{ fontSize: "0.8em", marginBottom: 4 }}>
        {title} ({entries.length})
      </div>
      {entries.length === 0 ? (
        <div className="dim" style={{ fontSize: "0.82em" }}>
          —
        </div>
      ) : (
        <div className="chips">
          {entries.map(([k, v]) => (
            <span
              className="chip mono"
              key={k}
              title={`${k}=${v} (click to copy)`}
              style={{ cursor: "copy" }}
              onClick={() => copyText(`${k}=${v}`)}
            >
              {k}={truncate(v, 40)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}
