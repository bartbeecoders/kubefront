import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../api";
import type { PodRow, ResourceDetail, Selection } from "../types";
import { statusClass } from "../views";

interface Props {
  selected: Selection | null;
  pods: PodRow[];
  onOpenLogs: (pod: PodRow) => void;
}

export function DetailPanel({ selected, pods, onOpenLogs }: Props) {
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
    // Re-fetch whenever the selected identity changes.
  }, [selected?.kind, selected?.name, selected?.namespace]);

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

  return (
    <aside className="detail">
      <h2 title={selected.name}>{selected.name}</h2>
      <div className="dim mono">
        {selected.kind}
        {selected.namespace ? ` · ns/${selected.namespace}` : ""}
      </div>

      {pod && (
        <button className="btn primary" style={{ marginTop: 12 }} onClick={() => onOpenLogs(pod)}>
          📜 View Logs (live)
        </button>
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
          <button
            className="btn sm"
            style={{ marginTop: 12 }}
            onClick={() => setShowManifest((s) => !s)}
          >
            {showManifest ? "Hide" : "Show"} manifest
          </button>
          {showManifest && <pre className="manifest">{detail.manifest}</pre>}
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
            <span className="chip mono" key={k} title={`${k}=${v}`}>
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
