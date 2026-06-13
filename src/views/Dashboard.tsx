import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { AppState, ClusterSummary, KubeStatus } from "../types";
import { CopyButton } from "../components/CopyButton";

interface Props {
  settings: AppState;
  status: KubeStatus;
  /** Connect to (kubeconfig path, context) and navigate to the cluster details. */
  onOpen: (path: string | null, context: string) => void;
  /** Connect to a remote backend connection by id. */
  onSelectRemote: (id: string) => void;
}

/** One card: either a Direct (kubeconfig file, context) pair or a Remote backend. */
interface ClusterCard {
  key: string;
  kind: "direct" | "remote";
  /** Display title — the context name (Direct) or the connection name (Remote). */
  title: string;
  /** Server URL (Direct) or backend endpoint (Remote). */
  server: string;
  isK3s: boolean;
  /** Friendly name of the source connection entry. */
  source: string;
  /** Per-connection namespace used to scope pod/deployment counts. */
  namespace: string | null;
  // Direct only:
  path?: string | null;
  context?: string;
  // Remote only:
  remoteId?: string;
}

type SummaryState = { loading: boolean; data: ClusterSummary | null };

const UNREACHABLE = (error: string): ClusterSummary => ({
  reachable: false,
  version: null,
  nodes_total: null,
  nodes_ready: null,
  pods_total: null,
  pods_running: null,
  namespaces: null,
  deployments: null,
  error,
});

export function DashboardView({ settings, status, onOpen, onSelectRemote }: Props) {
  const [cards, setCards] = useState<ClusterCard[]>([]);
  const [enumerating, setEnumerating] = useState(true);
  const [summaries, setSummaries] = useState<Record<string, SummaryState>>({});

  // Stable identity for the registered-connection list so effects don't loop.
  const entriesKey = useMemo(
    () =>
      settings.kubeconfigs
        .map((e) => `${e.mode}|${e.id}|${e.path}|${e.endpoint ?? ""}|${e.namespace ?? ""}`)
        .join("\n"),
    [settings.kubeconfigs],
  );

  const probe = useCallback((card: ClusterCard) => {
    setSummaries((s) => ({ ...s, [card.key]: { loading: true, data: s[card.key]?.data ?? null } }));
    const req =
      card.kind === "remote"
        ? api.remoteSummary(card.remoteId as string)
        : api.clusterSummary(card.path ?? null, card.context as string, card.namespace);
    req
      .then((data) => setSummaries((s) => ({ ...s, [card.key]: { loading: false, data } })))
      .catch((e) =>
        setSummaries((s) => ({ ...s, [card.key]: { loading: false, data: UNREACHABLE(String(e)) } })),
      );
  }, []);

  // Enumerate cards: one per remote connection, plus every context of every Direct
  // kubeconfig. Fall back to the loaded kubeconfig only when nothing is registered.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      setEnumerating(true);
      const next: ClusterCard[] = [];
      const seen = new Set<string>();

      // Remote connections: one card each (no context fan-out).
      for (const e of settings.kubeconfigs) {
        if (e.mode !== "Remote") continue;
        const key = `remote::${e.id}`;
        if (seen.has(key)) continue;
        seen.add(key);
        next.push({
          key,
          kind: "remote",
          title: e.name,
          server: e.endpoint ?? "",
          isK3s: false,
          source: e.name,
          namespace: e.namespace,
          remoteId: e.id,
        });
      }

      // Direct kubeconfigs: every context. Fall back to the loaded kubeconfig only
      // when no connections are registered at all.
      const directEntries = settings.kubeconfigs.filter((e) => e.mode !== "Remote");
      const entries = directEntries.length
        ? directEntries.map((e) => ({
            path: e.path as string | null,
            source: e.name,
            namespace: e.namespace,
          }))
        : settings.kubeconfigs.length
          ? []
          : [{ path: status.kubeconfig_path, source: "Loaded kubeconfig", namespace: null }];

      for (const entry of entries) {
        try {
          const contexts =
            entry.path === status.kubeconfig_path && status.contexts.length
              ? status.contexts
              : await api.kubeconfigContexts(entry.path);
          for (const ctx of contexts) {
            const key = `${entry.path ?? ""}::${ctx.name}`;
            if (seen.has(key)) continue;
            seen.add(key);
            next.push({
              key,
              kind: "direct",
              title: ctx.name,
              server: ctx.server,
              isK3s: ctx.is_k3s,
              source: entry.source,
              namespace: entry.namespace,
              path: entry.path,
              context: ctx.name,
            });
          }
        } catch {
          // Unreadable kubeconfig file — skip its cards rather than fail the page.
        }
      }
      if (cancelled) return;
      setCards(next);
      setEnumerating(false);
      for (const c of next) probe(c);
    })();
    return () => {
      cancelled = true;
    };
    // Re-enumerate when the registered list or the loaded kubeconfig changes.
  }, [entriesKey, status.kubeconfig_path, probe]);

  const isActive = (c: ClusterCard) =>
    c.kind === "remote"
      ? status.connected && settings.active_kubeconfig_id === c.remoteId
      : status.connected &&
        status.current_context === c.context &&
        (c.path == null || c.path === status.kubeconfig_path);

  const openCard = (c: ClusterCard) =>
    c.kind === "remote"
      ? onSelectRemote(c.remoteId as string)
      : onOpen(c.path ?? null, c.context as string);

  return (
    <div>
      <div className="page-head">
        <h1>Dashboard</h1>
        <span className="count">
          ({cards.length} cluster{cards.length === 1 ? "" : "s"})
        </span>
        <button
          className="btn sm"
          style={{ marginLeft: "auto" }}
          onClick={() => cards.forEach(probe)}
          disabled={enumerating || !cards.length}
        >
          ⟳ Refresh all
        </button>
      </div>

      {enumerating && !cards.length ? (
        <div className="empty">Discovering clusters…</div>
      ) : !cards.length ? (
        <div className="empty">
          No clusters configured. Add a kubeconfig or a remote connection in Settings.
        </div>
      ) : (
        <div className="cluster-grid">
          {cards.map((c) => (
            <ClusterCardView
              key={c.key}
              card={c}
              active={isActive(c)}
              summary={summaries[c.key]}
              onOpen={() => openCard(c)}
              onRefresh={() => probe(c)}
            />
          ))}
        </div>
      )}

      <div className="hint">
        Direct kubeconfig contexts and remote backend connections • click a card to connect
      </div>
    </div>
  );
}

function ClusterCardView({
  card,
  active,
  summary,
  onOpen,
  onRefresh,
}: {
  card: ClusterCard;
  active: boolean;
  summary: SummaryState | undefined;
  onOpen: () => void;
  onRefresh: () => void;
}) {
  const s = summary?.data;
  const loading = summary?.loading ?? true;
  const dot = loading ? "pending" : s?.reachable ? "running" : "failed";
  const stateLabel = loading ? "Checking…" : s?.reachable ? "Online" : "Unreachable";

  const fmt = (v: number | null | undefined) => (v == null ? "—" : String(v));
  const pair = (a: number | null | undefined, b: number | null | undefined) =>
    a == null || b == null ? "—" : `${a}/${b}`;

  return (
    <div
      className={`cluster-card${active ? " active" : ""}`}
      onClick={onOpen}
      title={`Open ${card.title}`}
    >
      <div className="cluster-card-head">
        <span className={`dot ${dot}`} />
        <span className="cluster-name" title={card.title}>
          {card.title}
        </span>
        {card.kind === "remote" && <span className="pill pill-remote">Remote</span>}
        {card.isK3s && <span className="k3s-badge">K3S</span>}
        {active && <span className="active-badge">connected</span>}
        <button
          className="btn ghost sm"
          title="Refresh this cluster"
          onClick={(e) => {
            e.stopPropagation();
            onRefresh();
          }}
        >
          ⟳
        </button>
      </div>

      <div className="cluster-server">
        <span className="mono" title={card.server}>
          {card.server || "—"}
        </span>
        {card.server && <CopyButton text={card.server} title="Copy server URL" />}
      </div>

      <div className="cluster-meta dim">
        {card.source}
        {card.namespace ? ` · ns/${card.namespace}` : ""}
        {" · "}
        <span className={`cluster-state ${dot}`}>{stateLabel}</span>
        {s?.version ? ` · ${s.version}` : ""}
      </div>

      {s && !s.reachable && s.error ? (
        <div className="cluster-error" title={s.error}>
          {s.error}
        </div>
      ) : (
        <div className="cluster-stats">
          <div className="cstat">
            <div className="v">{pair(s?.nodes_ready, s?.nodes_total)}</div>
            <div className="l">Nodes ready</div>
          </div>
          <div className="cstat">
            <div className="v">{pair(s?.pods_running, s?.pods_total)}</div>
            <div className="l">Pods running</div>
          </div>
          <div className="cstat">
            <div className="v">{fmt(s?.deployments)}</div>
            <div className="l">Deployments</div>
          </div>
          <div className="cstat">
            <div className="v">{fmt(s?.namespaces)}</div>
            <div className="l">Namespaces</div>
          </div>
        </div>
      )}
    </div>
  );
}
