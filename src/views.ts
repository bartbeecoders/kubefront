// Navigation structure and declarative table-view definitions.
// Generic list pages are data-driven (a kind + a title); only Clusters, Pods,
// Nodes, Monitoring, Logging and Settings need bespoke components.

export type ViewKey =
  | "clusters"
  | "nodes"
  | "namespaces"
  | "pods"
  | "deployments"
  | "statefulsets"
  | "daemonsets"
  | "jobs"
  | "cronjobs"
  | "configmaps"
  | "secrets"
  | "services"
  | "storage"
  | "network"
  | "security"
  | "monitoring"
  | "logging"
  | "settings";

export interface NavItem {
  key: ViewKey;
  label: string;
  icon: string;
}

export interface NavSection {
  heading: string;
  items: NavItem[];
}

export const NAV: NavSection[] = [
  {
    heading: "Cluster",
    items: [
      { key: "clusters", label: "Clusters", icon: "🖧" },
      { key: "nodes", label: "Nodes", icon: "🖥" },
      { key: "namespaces", label: "Namespaces", icon: "🗂" },
    ],
  },
  {
    heading: "Workloads",
    items: [
      { key: "pods", label: "Pods", icon: "📦" },
      { key: "deployments", label: "Deployments", icon: "🚀" },
      { key: "statefulsets", label: "StatefulSets", icon: "🧱" },
      { key: "daemonsets", label: "DaemonSets", icon: "👥" },
      { key: "jobs", label: "Jobs", icon: "⚙" },
      { key: "cronjobs", label: "CronJobs", icon: "⏰" },
    ],
  },
  {
    heading: "Config",
    items: [
      { key: "configmaps", label: "ConfigMaps", icon: "📄" },
      { key: "secrets", label: "Secrets", icon: "🔒" },
    ],
  },
  {
    heading: "Storage & Network",
    items: [
      { key: "services", label: "Services", icon: "🔌" },
      { key: "storage", label: "Storage", icon: "💾" },
      { key: "network", label: "Network", icon: "🌐" },
    ],
  },
  {
    heading: "Access",
    items: [{ key: "security", label: "Security", icon: "🛡" }],
  },
  {
    heading: "Observability",
    items: [
      { key: "monitoring", label: "Monitoring", icon: "📊" },
      { key: "logging", label: "Logging", icon: "📜" },
    ],
  },
  {
    heading: "App",
    items: [{ key: "settings", label: "Settings", icon: "⚙" }],
  },
];

export interface TableSection {
  title: string;
  kind: string;
  empty: string;
}

export interface TableView {
  title: string;
  sections: TableSection[];
}

/** Views that are just one or more resource tables. */
export const TABLE_VIEWS: Partial<Record<ViewKey, TableView>> = {
  namespaces: {
    title: "Namespaces",
    sections: [{ title: "Namespaces", kind: "namespaces", empty: "No namespaces found." }],
  },
  deployments: {
    title: "Deployments",
    sections: [{ title: "Deployments", kind: "deployments", empty: "No deployments in scope." }],
  },
  statefulsets: {
    title: "StatefulSets",
    sections: [{ title: "StatefulSets", kind: "statefulsets", empty: "No statefulsets in scope." }],
  },
  daemonsets: {
    title: "DaemonSets",
    sections: [{ title: "DaemonSets", kind: "daemonsets", empty: "No daemonsets in scope." }],
  },
  jobs: {
    title: "Jobs",
    sections: [{ title: "Jobs", kind: "jobs", empty: "No jobs in scope." }],
  },
  cronjobs: {
    title: "CronJobs",
    sections: [{ title: "CronJobs", kind: "cronjobs", empty: "No cronjobs in scope." }],
  },
  configmaps: {
    title: "ConfigMaps",
    sections: [{ title: "ConfigMaps", kind: "configmaps", empty: "No configmaps in scope." }],
  },
  secrets: {
    title: "Secrets",
    sections: [{ title: "Secrets", kind: "secrets", empty: "No secrets in scope." }],
  },
  services: {
    title: "Services",
    sections: [{ title: "Services", kind: "services", empty: "No services in scope." }],
  },
  storage: {
    title: "Storage",
    sections: [
      { title: "Persistent Volume Claims", kind: "pvcs", empty: "No PVCs in scope." },
      { title: "Persistent Volumes", kind: "pvs", empty: "No persistent volumes found." },
      { title: "Storage Classes", kind: "storageclasses", empty: "No storage classes found." },
    ],
  },
  network: {
    title: "Network",
    sections: [
      { title: "Services", kind: "services", empty: "No services in scope." },
      { title: "Ingresses", kind: "ingresses", empty: "No ingresses in scope." },
      { title: "Network Policies", kind: "networkpolicies", empty: "No network policies in scope." },
    ],
  },
  security: {
    title: "Access Control",
    sections: [
      { title: "Service Accounts", kind: "serviceaccounts", empty: "No service accounts in scope." },
      { title: "Roles", kind: "roles", empty: "No roles in scope." },
      { title: "Role Bindings", kind: "rolebindings", empty: "No role bindings in scope." },
    ],
  },
};

/** Status string → pill CSS class. */
export function statusClass(s: string): string {
  switch (s) {
    case "Running":
    case "Ready":
    case "Active":
    case "Bound":
      return "running";
    case "Pending":
    case "ContainerCreating":
      return "pending";
    case "Succeeded":
    case "Completed":
      return "succeeded";
    case "Failed":
    case "NotReady":
      return "failed";
    default:
      return "";
  }
}
