import type { AppState, KubeStatus } from "../types";

interface Props {
  status: KubeStatus;
  settings: AppState | null;
  podCount: number;
}

export function StatusBar({ status, settings, podCount }: Props) {
  const activeName =
    settings?.kubeconfigs.find((k) => k.id === settings.active_kubeconfig_id)?.name ??
    status.kubeconfig_path ??
    "~/.kube/config (or $KUBECONFIG)";
  const isRemote =
    !!status.kubeconfig_path && /^https?:\/\//.test(status.kubeconfig_path);

  return (
    <footer className="statusbar">
      <span>Connection: {activeName}</span>
      <span className="sep">•</span>
      {isRemote ? (
        <span style={{ color: "var(--accent)" }}>Remote backend</span>
      ) : status.context_count === 0 ? (
        <span style={{ color: "var(--status-failed)" }}>0 contexts found</span>
      ) : (
        <span>{status.context_count} contexts</span>
      )}
      <span className="sep">•</span>
      <span>{podCount} pods</span>
      <div className="spacer" />
      <span>Tauri + React • kube-rs • K3S friendly</span>
    </footer>
  );
}
