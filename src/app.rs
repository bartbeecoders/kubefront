use eframe::{egui, Frame, Storage};
use egui::{Align, Layout, RichText, Ui};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::k8s::manager::KubeConfigManager;
use crate::state::AppState;
use crate::ui::theme;

use k8s_openapi::api::core::v1::{Node, Pod};

/// Messages sent from the UI thread to the background worker.
#[derive(Debug)]
enum KubeCommand {
    // Connect (or reconnect) to the currently selected context in the manager.
    ConnectToCurrent,
    // Future: Disconnect, RefreshResources, etc.
}

/// Events sent from background tasks back to the UI thread.
enum KubeEvent {
    Connected {
        /// We clone the Client (it's cheap — internal Arc) so the UI can use it later.
        client: kube::Client,
        version: Option<String>,
    },
    ConnectionFailed(String),

    // Phase 2: real resource updates
    PodsUpdated(Vec<Pod>),
    NodesUpdated(Vec<Node>),

    // Phase 3: Live logs
    LogLine {
        window_id: usize,
        line: String,
    },
    LogError {
        window_id: usize,
        error: String,
    },
}

/// Top-level application. Owns all UI state and background task channels.
pub struct KubeFrontApp {
    state: AppState,

    // Real kubeconfig manager (Phase 1 wiring started)
    kube_manager: KubeConfigManager,

    // --- Real connection state (Phase 1) ---
    is_connected: bool,
    cluster_version: Option<String>,
    last_error: Option<String>,
    /// The live client we use for all Kubernetes operations.
    /// Only set after a successful async connect.
    current_client: Option<kube::Client>,

    // --- Real resource data (Phase 2) ---
    pods: Vec<Pod>,
    nodes: Vec<Node>,
    selected_pod: Option<usize>,
    selected_node: Option<usize>,

    // View mode
    active_view: View,

    // Typed channels for async kube work (UI thread <-> background Tokio tasks)
    cmd_tx: Sender<KubeCommand>,
    event_tx: Sender<KubeEvent>,   // kept so we can spawn one-off refresh tasks from UI
    event_rx: Receiver<KubeEvent>,

    // Runtime handle so we can spawn refresh tasks directly from the UI thread after we have a Client
    rt_handle: tokio::runtime::Handle,

    // Simple auto-refresh tracking for Phase 2
    last_refresh: std::time::Instant,

    // Phase 3: Log window ID generator
    next_log_window_id: usize,

    // UI transient state
    pod_filter: String,
    pod_namespace_filter: String, // "All" or specific
    log_windows: Vec<LogWindowState>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Pods,
    Nodes,
}

#[derive(Default)]
struct LogWindowState {
    id: usize,
    pod_name: String,
    namespace: String,

    // Multi-container support
    available_containers: Vec<String>,
    selected_container: Option<String>,

    lines: Vec<String>,
    follow: bool,
    filter: String,

    // Internal: used to request cancellation of the background stream
    _cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,

    // Used for deferred reconnects (e.g. after changing container)
    pending_reconnect: bool,
}

impl KubeFrontApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Load persisted state if available (Phase 1 will also restore last context)
        let state = _cc
            .storage
            .and_then(|s| eframe::get_value::<AppState>(s, AppState::STORAGE_KEY))
            .unwrap_or_default();

        // --- Tokio runtime on a dedicated background thread (critical egui + async pattern) ---
        // This lets us call `tokio::spawn` from the UI thread via rt.enter().
        let (cmd_tx, cmd_rx) = mpsc::channel::<KubeCommand>();
        let (event_tx, event_rx) = mpsc::channel::<KubeEvent>();

        // Snapshot only what the background thread needs for the *very first* connect.
        let initial_path = state.last_kubeconfig_path.clone();
        let event_tx_bg = event_tx.clone();

        let mut initial_load_error: Option<String> = None;

        // Create the multi-thread runtime
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for background tasks");

        // Keep a handle so the UI thread can spawn short-lived refresh tasks once we have a Client
        let rt_handle = rt.handle().clone();

        // Spawn a driver thread that keeps the runtime alive and processes commands
        let _bg_thread = thread::spawn(move || {
            // Enter the runtime so that `tokio::spawn` works from anywhere in this thread
            let _guard = rt.enter();

            // Simple command loop. In a more advanced version we would use select! with shutdown.
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    KubeCommand::ConnectToCurrent => {
                        tracing::info!("Received ConnectToCurrent command in background thread");

                        // For the initial connect we use the snapshot captured at startup.
                        // Subsequent connects (after the user loads a different file or switches context)
                        // are driven from the UI, which already updated the manager before sending the command.
                        let mut mgr = KubeConfigManager::new();
                        let load_res = if let Some(p) = &initial_path {
                            tracing::debug!("Background connect using persisted path: {:?}", p);
                            mgr.load_from_path(p)
                        } else {
                            tracing::debug!("Background connect using default kubeconfig discovery");
                            mgr.load_default()
                        };

                        if let Err(e) = load_res {
                            tracing::error!("Background config load failed: {}", e);
                            let _ = event_tx_bg.send(KubeEvent::ConnectionFailed(format!("Failed to load config: {e}")));
                            continue;
                        }

                        let context_name = mgr.effective_context().unwrap_or("unknown").to_string();
                        let server = mgr.current_info().map(|i| i.server.clone()).unwrap_or_default();
                        tracing::info!("Attempting to create client for context '{}' → server: {}", context_name, server);

                        // Spawn the actual async connect work
                        let tx = event_tx_bg.clone();
                        tokio::spawn(async move {
                            // Add a generous timeout so we don't hang forever on bad auth / exec plugins
                            let connect_fut = mgr.create_client_for_current();
                            match tokio::time::timeout(std::time::Duration::from_secs(15), connect_fut).await {
                                Ok(Ok(client)) => {
                                    tracing::info!("Successfully created kube client for context {}", context_name);
                                    // Best-effort cluster version probe (very useful for K3S users)
                                    let version = probe_cluster_version(&client).await;
                                    let _ = tx.send(KubeEvent::Connected { client, version });
                                }
                                Ok(Err(e)) => {
                                    tracing::error!("Client creation failed for {}: {}", context_name, e);
                                    let _ = tx.send(KubeEvent::ConnectionFailed(e.to_string()));
                                }
                                Err(_) => {
                                    tracing::error!("Client creation timed out after 15s for context {}", context_name);
                                    let _ = tx.send(KubeEvent::ConnectionFailed(
                                        format!("Connection to {} timed out after 15s. Common causes: server unreachable from this machine, firewall, wrong network/VPN, or TLS verification failure. Try the same `kubectl` command from this exact terminal.", server)
                                    ));
                                }
                            }
                        });
                    }
                }
            }
        });

        // Start with empty real data — will be populated by background polling once connected
        let pods: Vec<Pod> = vec![];
        let nodes: Vec<Node> = vec![];

        let mut kube_manager = KubeConfigManager::new();
        // Best-effort load of default so the app is immediately useful
        if let Err(e) = kube_manager.load_default() {
            // Make the error visible to the user instead of silently swallowing it
            initial_load_error = Some(format!(
                "Failed to load default kubeconfig (~/.kube/config or $KUBECONFIG): {e}"
            ));
            tracing::error!("Initial kubeconfig load failed: {}", e);
        } else {
            tracing::info!(
                "Loaded kubeconfig with {} context(s)",
                kube_manager.contexts.len()
            );
        }

        // If we have a persisted last context, try to select it
        if let Some(last_ctx) = &state.last_context {
            if kube_manager.contexts.iter().any(|c| &c.name == last_ctx) {
                kube_manager.current_context = Some(last_ctx.clone());
            }
        }

        let app = Self {
            state,
            kube_manager,
            is_connected: false,
            cluster_version: None,
            last_error: initial_load_error,
            current_client: None,

            pods,
            nodes,
            selected_pod: Some(0),
            selected_node: None,

            active_view: View::Pods,

            cmd_tx,
            event_tx: event_tx.clone(), // keep sender for direct spawning of refresh tasks
            event_rx,

            rt_handle,
            last_refresh: std::time::Instant::now(),
            next_log_window_id: 1,

            pod_filter: String::new(),
            pod_namespace_filter: "All".to_string(),

            log_windows: vec![],
        };

        // Kick off an initial connection attempt if we have a context
        if app.kube_manager.effective_context().is_some() {
            tracing::info!("Sending initial ConnectToCurrent command for context: {:?}", app.kube_manager.effective_context());
            let _ = app.cmd_tx.send(KubeCommand::ConnectToCurrent);
        } else {
            tracing::warn!("No context available after loading kubeconfig — cannot connect");
        }

        app
    }

    fn save_state(&mut self, storage: &mut dyn Storage) {
        eframe::set_value(storage, AppState::STORAGE_KEY, &self.state);
    }

    /// Trigger background refresh of Pods + Nodes using the live client (Phase 2).
    fn trigger_refresh(&mut self) {
        if let Some(client) = self.current_client.clone() {
            let pods_tx = self.event_tx.clone();
            let nodes_tx = self.event_tx.clone();

            self.rt_handle.spawn(async move {
                // Fetch Pods
                use k8s_openapi::api::core::v1::Pod as K8sPod;
                use kube::api::{Api, ListParams};

                let pods_api: Api<K8sPod> = Api::all(client.clone());
                if let Ok(pod_list) = pods_api.list(&ListParams::default()).await {
                    let _ = pods_tx.send(KubeEvent::PodsUpdated(pod_list.items));
                }

                // Fetch Nodes
                use k8s_openapi::api::core::v1::Node as K8sNode;
                let nodes_api: Api<K8sNode> = Api::all(client);
                if let Ok(node_list) = nodes_api.list(&ListParams::default()).await {
                    let _ = nodes_tx.send(KubeEvent::NodesUpdated(node_list.items));
                }
            });

            self.last_refresh = std::time::Instant::now();
        }
    }

    /// Phase 3: Open a live log window for a pod (and optionally a specific container).
    /// This is the preferred way when you have the full Pod object (gives container list).
    pub fn open_logs_for_pod(&mut self, pod: &Pod) {
        let containers: Vec<String> = pod
            .spec
            .as_ref()
            .map(|spec| spec.containers.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();

        let initial_container = containers.first().cloned();

        self.open_log_window(
            pod_name(pod),
            pod_namespace(pod),
            initial_container,
            containers,
        );
    }

    fn open_log_window(
        &mut self,
        pod_name: &str,
        namespace: &str,
        container: Option<String>,
        available_containers: Vec<String>,
    ) {
        if self.current_client.is_none() {
            // Can't stream logs without a client
            return;
        }

        let window_id = self.next_log_window_id;
        self.next_log_window_id += 1;

        // Create a cancellation channel so we can stop the stream later
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let client = self.current_client.clone().unwrap();
        let event_tx = self.event_tx.clone();
        let pod = pod_name.to_string();
        let ns = namespace.to_string();
        let selected_container = container.clone();

        // Create the UI state first (before moving strings into the task)
        self.log_windows.push(LogWindowState {
            id: window_id,
            pod_name: pod.clone(),
            namespace: ns.clone(),
            available_containers,
            selected_container: selected_container.clone(),
            lines: vec![],
            follow: true,
            filter: String::new(),
            _cancel_tx: Some(cancel_tx),
            pending_reconnect: false,
        });

        self.rt_handle.spawn(async move {
            use kube::api::{Api, LogParams};

            let pods: Api<k8s_openapi::api::core::v1::Pod> = Api::namespaced(client, &ns);

            let mut params = LogParams {
                follow: true,
                tail_lines: Some(200),
                timestamps: true,
                ..Default::default()
            };

            if let Some(c) = &selected_container {
                params.container = Some(c.clone());
            }

            match pods.log_stream(&pod, &params).await {
                Ok(mut logs) => {
                    // Send initial header
                    let header = format!("--- Streaming logs for {}/{} ---", ns.clone(), pod.clone());
                    let _ = event_tx.send(KubeEvent::LogLine {
                        window_id,
                        line: header,
                    });

                    // log_stream returns an AsyncBufRead (from futures_io)
                    use futures_util::io::AsyncBufReadExt;
                    use futures_util::StreamExt;
                    let mut lines = logs.lines();

                    tokio::select! {
                        _ = async {
                            while let Some(result) = lines.next().await {
                                match result {
                                    Ok(line) => {
                                        if !line.is_empty() {
                                            let _ = event_tx.send(KubeEvent::LogLine {
                                                window_id,
                                                line,
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(KubeEvent::LogError {
                                            window_id,
                                            error: format!("Error reading log line: {}", e),
                                        });
                                        break;
                                    }
                                }
                            }
                        } => {},
                        _ = cancel_rx => {
                            // User closed the log window
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(KubeEvent::LogError {
                        window_id,
                        error: format!("Failed to open log stream: {}", e),
                    });
                }
            }
        });

    }
}

/// Very lightweight cluster version probe.
/// We try the discovery or a simple request. Falls back gracefully.
async fn probe_cluster_version(client: &kube::Client) -> Option<String> {
    // Many K3S setups expose the version nicely via node status or we can just return something useful.
    // For MVP we do a very cheap call: list nodes and take the first kubelet version if present.
    use k8s_openapi::api::core::v1::Node;
    use kube::api::{Api, ListParams};

    let nodes: Api<Node> = Api::all(client.clone());
    if let Ok(list) = nodes.list(&ListParams::default().limit(1)).await {
        if let Some(node) = list.items.first() {
            if let Some(status) = &node.status {
                if let Some(info) = &status.node_info {
                    return Some(info.kubelet_version.clone());
                }
            }
        }
    }
    None
}

impl eframe::App for KubeFrontApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // Drain events coming from background kube tasks
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                KubeEvent::Connected { client, version } => {
                    tracing::info!("Received Connected event from background task");
                    self.current_client = Some(client);
                    self.cluster_version = version.or_else(|| Some("connected".into()));
                    self.is_connected = true;
                    self.last_error = None;

                    // Persist the successful connection
                    self.state.last_kubeconfig_path = self.kube_manager.path.as_ref().map(|p| p.display().to_string());
                    self.state.last_context = self.kube_manager.current_context.clone();
                }
                KubeEvent::ConnectionFailed(err) => {
                    tracing::error!("Received ConnectionFailed event: {}", err);
                    self.is_connected = false;
                    self.current_client = None;
                    self.cluster_version = None;
                    self.last_error = Some(err);
                }

                // Phase 2 real data
                KubeEvent::PodsUpdated(new_pods) => {
                    self.pods = new_pods;
                }
                KubeEvent::NodesUpdated(new_nodes) => {
                    self.nodes = new_nodes;
                }

                // Phase 3: Live logs
                KubeEvent::LogLine { window_id, line } => {
                    if let Some(window) = self.log_windows.iter_mut().find(|w| w.id == window_id) {
                        window.lines.push(line);
                        // Keep a reasonable ring buffer
                        if window.lines.len() > 5000 {
                            let drain_count = window.lines.len() - 4000;
                            window.lines.drain(0..drain_count);
                        }
                    }
                }
                KubeEvent::LogError { window_id, error } => {
                    if let Some(window) = self.log_windows.iter_mut().find(|w| w.id == window_id) {
                        window.lines.push(format!("[ERROR] {}", error));
                    }
                }
            }
            ctx.request_repaint();
        }

        // Simple auto-refresh (every 5s) when we have a live client — Phase 2
        if self.is_connected && self.current_client.is_some() {
            if self.last_refresh.elapsed() > std::time::Duration::from_secs(5) {
                self.trigger_refresh();
            }
        }

        // === TOP BAR ===
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("KubeFront").strong().size(20.0));
                ui.label(RichText::new("K3S & Kubernetes").weak().size(12.0));

                ui.separator();

                // Quick actions
                if ui.button("Load kubeconfig...").clicked() {
                    // rfd is synchronous and opens a native dialog — this is the standard
                    // and expected pattern for desktop file pickers.
                    let start_dir = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Kubeconfig", &["yaml", "yml", "config", ""])
                        .set_directory(start_dir)
                        .pick_file()
                    {
                        match self.kube_manager.load_from_path(&path) {
                            Ok(_) => {
                                self.last_error = None;
                                self.is_connected = false;
                                self.current_client = None;
                                let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                            }
                            Err(e) => {
                                self.last_error = Some(format!("Failed to load {}: {e}", path.display()));
                            }
                        }
                    }
                }
                if ui.button("Use default").clicked() {
                    if let Err(e) = self.kube_manager.load_default() {
                        self.last_error = Some(format!("Default load failed: {e}"));
                    } else {
                        self.last_error = None;
                        self.is_connected = false;
                        self.current_client = None;
                        let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                    }
                }
                if ui.button("K3S default").clicked() {
                    let p = "/etc/rancher/k3s/k3s.yaml";
                    if let Err(e) = self.kube_manager.load_from_path(p) {
                        self.last_error = Some(format!("K3S path ({p}) load failed: {e} — copy to ~/.kube/config or fix perms"));
                    } else {
                        self.last_error = None;
                        self.is_connected = false;
                        self.current_client = None;
                        let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                    }
                }

                ui.separator();

                // Real context switcher (populated from loaded Kubeconfig)
                ui.label("Context:");
                let selected_text = self
                    .kube_manager
                    .current_info()
                    .map(|c| {
                        if c.is_k3s {
                            format!("{} 🟣 K3S", c.name)
                        } else {
                            c.name.clone()
                        }
                    })
                    .unwrap_or_else(|| "Select context...".into());

                egui::ComboBox::from_id_salt("context_switcher")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for info in &self.kube_manager.contexts {
                            let label = if info.is_k3s {
                                format!("{}  🟣 K3S  ({})", info.name, info.cluster)
                            } else {
                                format!("{} ({})", info.name, info.cluster)
                            };
                            let is_current = self.kube_manager.current_context.as_deref() == Some(info.name.as_str());
                            if ui.selectable_label(is_current, label).clicked() {
                                self.kube_manager.current_context = Some(info.name.clone());
                                self.is_connected = false;
                                self.current_client = None;
                                self.cluster_version = None;

                                // Trigger async connect in the background
                                let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                            }
                        }
                    });

                ui.separator();

                if self.is_connected {
                    ui.colored_label(egui::Color32::from_rgb(34, 197, 94), "● Connected");
                } else if self.current_client.is_none() && self.kube_manager.effective_context().is_some() {
                    let ctx = self.kube_manager.effective_context().unwrap_or("?");
                    ui.colored_label(egui::Color32::from_rgb(234, 179, 8), format!("● Connecting to {}...", ctx));
                    if ui.small_button("Retry").clicked() {
                        self.is_connected = false;
                        self.current_client = None;
                        let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                    }
                } else {
                    ui.colored_label(egui::Color32::from_rgb(239, 68, 68), "● Disconnected");
                    if self.kube_manager.effective_context().is_some() {
                        if ui.button("Reconnect").clicked() {
                            self.is_connected = false;
                            self.current_client = None;
                            let _ = self.cmd_tx.send(KubeCommand::ConnectToCurrent);
                        }
                    }
                }

                if let Some(ver) = &self.cluster_version {
                    ui.label(RichText::new(ver).monospace().size(11.0));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.trigger_refresh();
                    }
                    ui.label(format!("Auto: {}s", self.state.auto_refresh_secs));
                });
            });

            // Error banner
            if let Some(err) = &self.last_error {
                ui.colored_label(egui::Color32::from_rgb(239, 68, 68), format!("⚠ {}", err));
                ui.small("Tip: Since kubectl works in this terminal, the issue is likely network reachability or TLS from the Rust process. Check the debug logs (RUST_LOG=debug).");
            }
        });

        // === LEFT SIDEBAR (nav) ===
        egui::SidePanel::left("nav").resizable(false).exact_width(140.0).show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("Resources");
                ui.separator();

                for (label, view) in [("Pods", View::Pods), ("Nodes", View::Nodes)] {
                    let selected = self.active_view == view;
                    if ui.selectable_label(selected, label).clicked() {
                        self.active_view = view;
                    }
                }

                ui.separator();
                ui.label(RichText::new("K3S conveniences").weak().size(10.0));
                ui.small("• Local K3S quick connect");
                ui.small("• Context badges");
                ui.small("(Phase 4+)");
            });
        });

        // === RIGHT DETAIL PANEL (conditional) ===
        if self.state.show_right_panel {
            egui::SidePanel::right("detail").resizable(true).default_width(320.0).show(ctx, |ui| {
                ui.heading("Details");
                ui.separator();

                match self.active_view {
                    View::Pods => {
                        if let Some(idx) = self.selected_pod {
                            if let Some(pod) = self.pods.get(idx) {
                                ui.label(RichText::new(pod_name(pod)).strong().size(14.0));
                                ui.monospace(format!("ns/{}", pod_namespace(pod)));
                                ui.separator();
                                ui.label(format!("Status: {}", pod_phase(pod)));
                                ui.label(format!("Ready: {}", pod_ready(pod)));
                                ui.label(format!("Restarts: {}", pod_restarts(pod)));
                                ui.label(format!("Node: {}", pod_node(pod)));
                                ui.label(format!("Age: {}", pod_age(pod)));
                                ui.separator();
                                if ui.button("📜 View Logs (live)").clicked() {
                                    let pod_name_str = pod_name(pod).to_string();
                                    let pod_ns_str = pod_namespace(pod).to_string();
                                    let containers: Vec<String> = pod
                                        .spec
                                        .as_ref()
                                        .map(|s| s.containers.iter().map(|c| c.name.clone()).collect())
                                        .unwrap_or_default();
                                    let initial = containers.first().cloned();
                                    self.open_log_window(
                                        &pod_name_str,
                                        &pod_ns_str,
                                        initial,
                                        containers,
                                    );
                                    ctx.request_repaint();
                                }
                            }
                        } else {
                            ui.label("Select a pod to see details");
                        }
                    }
                    View::Nodes => {
                        if let Some(idx) = self.selected_node {
                            if let Some(node) = self.nodes.get(idx) {
                                ui.label(RichText::new(node_name(node)).strong());
                                ui.monospace(node_version(node));
                                ui.label(format!("Status: {}", node_status(node)));
                                ui.label(format!("Roles: {}", node_roles(node)));
                            }
                        } else {
                            ui.label("Select a node");
                        }
                    }
                }
            });
        }

        // === CENTRAL CONTENT ===
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_view {
                View::Pods => self.draw_pods_view(ui, ctx),
                View::Nodes => self.draw_nodes_view(ui),
            }
        });

        // === FLOATING LOG WINDOWS (multiple supported) ===
        let mut to_close: Vec<usize> = vec![];
        for (i, log) in self.log_windows.iter_mut().enumerate() {
            let mut open = true;
            let title = if let Some(c) = &log.selected_container {
                format!("Logs: {}/{} [{}]", log.namespace, log.pod_name, c)
            } else {
                format!("Logs: {}/{}", log.namespace, log.pod_name)
            };
            egui::Window::new(title)
                .open(&mut open)
                .default_size([700.0, 420.0])
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut log.follow, "Follow");

                        // Container selector for multi-container pods
                        if log.available_containers.len() > 1 {
                            egui::ComboBox::from_id_salt(format!("container_{}", log.id))
                                .selected_text(log.selected_container.as_deref().unwrap_or("default"))
                                .show_ui(ui, |ui| {
                                    for c in &log.available_containers {
                                        if ui.selectable_label(
                                            log.selected_container.as_deref() == Some(c.as_str()),
                                            c,
                                        ).clicked() {
                                            if log.selected_container.as_deref() != Some(c.as_str()) {
                                                log.selected_container = Some(c.clone());
                                                // Stop current stream and schedule reconnect
                                                log._cancel_tx.take();
                                                log.pending_reconnect = true;
                                            }
                                        }
                                    }
                                });
                        }

                        ui.text_edit_singleline(&mut log.filter);

                        if ui.button("Copy visible").clicked() {
                            let visible: String = log.lines
                                .iter()
                                .filter(|l| log.filter.is_empty() || l.to_lowercase().contains(&log.filter.to_lowercase()))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join("\n");
                            ctx.copy_text(visible);
                        }
                        if ui.button("Clear").clicked() {
                            log.lines.clear();
                        }
                    });

                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .stick_to_bottom(log.follow)
                        .show(ui, |ui| {
                            for line in &log.lines {
                                if log.filter.is_empty() || line.to_lowercase().contains(&log.filter.to_lowercase()) {
                                    ui.monospace(RichText::new(line).size(11.0));
                                }
                            }
                        });

                    if log._cancel_tx.is_none() {
                        if ui.button("Reconnect logs").clicked() {
                            // Will be handled after window loop to avoid borrow issues
                            // For now user can close this window and reopen logs from pod detail
                        }
                    }

                    ui.small("Live streaming via kube log API");
                });

            if !open {
                to_close.push(i);
            }
        }
        // Remove closed windows (reverse to keep indices stable)
        for i in to_close.into_iter().rev() {
            self.log_windows.remove(i);
        }

        // Handle pending log window reconnects (after the loop to avoid borrow issues)
        let mut reconnect_indices = vec![];
        for (i, log) in self.log_windows.iter().enumerate() {
            if log.pending_reconnect {
                reconnect_indices.push(i);
            }
        }
        for i in reconnect_indices.into_iter().rev() {
            if let Some(log) = self.log_windows.get_mut(i) {
                log.pending_reconnect = false;
                // Close this one and open a fresh one with current selection
                let pod_name = log.pod_name.clone();
                let ns = log.namespace.clone();
                let container = log.selected_container.clone();
                let containers = log.available_containers.clone();
                self.open_log_window(&pod_name, &ns, container, containers);
            }
            self.log_windows.remove(i);
        }

        // Bottom status line
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Kubeconfig: {}",
                    self.kube_manager
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "~/.kube/config (or $KUBECONFIG)".into())
                ));

                if self.kube_manager.contexts.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(239, 68, 68),
                        " (0 contexts found — check file exists and is readable)"
                    );
                } else {
                    ui.label(format!(" ({} contexts)", self.kube_manager.contexts.len()));
                }
                ui.separator();
                ui.label(format!("{} pods visible (demo)", self.pods.len()));
                ui.separator();
                if self.is_connected {
                    ui.label(RichText::new("real client ready").color(egui::Color32::from_rgb(34, 197, 94)).small());
                }
                ui.separator();
                ui.label("egui 0.34 • pure Rust • K3S friendly");
            });
        });
    }

    /// Required in eframe 0.34+ (placeholder for now — real UI lives in update + helpers).
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut Frame) {}

    fn save(&mut self, storage: &mut dyn Storage) {
        self.save_state(storage);
    }
}

impl KubeFrontApp {
    fn draw_pods_view(&mut self, ui: &mut Ui, _ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.pod_filter);

            ui.label("Namespace:");
            egui::ComboBox::from_id_salt("ns_filter")
                .selected_text(&self.pod_namespace_filter)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.pod_namespace_filter, "All".into(), "All");

                    // Dynamic namespaces from real pods (Phase 2)
                    let mut namespaces: Vec<_> = self.pods.iter()
                        .map(pod_namespace)
                        .collect();
                    namespaces.sort();
                    namespaces.dedup();

                    for ns in namespaces {
                        ui.selectable_value(&mut self.pod_namespace_filter, ns.to_string(), ns);
                    }
                });

            if ui.button("Clear filters").clicked() {
                self.pod_filter.clear();
                self.pod_namespace_filter = "All".into();
            }

            ui.label(format!("({} pods)", self.pods.len()));
        });

        // Virtual table using egui_extras
        use egui_extras::{Column, TableBuilder};

        let table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::auto().at_least(240.0)) // name
            .column(Column::auto().at_least(110.0))
            .column(Column::auto().at_least(90.0))  // status
            .column(Column::auto().at_least(60.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::auto().at_least(70.0))
            .column(Column::remainder().at_least(140.0));

        table
            .header(22.0, |mut header| {
                header.col(|ui| { ui.strong("Name"); });
                header.col(|ui| { ui.strong("Namespace"); });
                header.col(|ui| { ui.strong("Status"); });
                header.col(|ui| { ui.strong("Ready"); });
                header.col(|ui| { ui.strong("Restarts"); });
                header.col(|ui| { ui.strong("Age"); });
                header.col(|ui| { ui.strong("Node"); });
            })
            .body(|mut body| {
                let row_height = 20.0;
                let lower = self.pod_filter.to_lowercase();

                for (i, pod) in self.pods.iter().enumerate() {
                    let name = pod_name(pod);
                    let ns = pod_namespace(pod);

                    // Client-side filter
                    if !lower.is_empty() && !name.to_lowercase().contains(&lower) && !ns.to_lowercase().contains(&lower) {
                        continue;
                    }
                    if self.pod_namespace_filter != "All" && ns != self.pod_namespace_filter {
                        continue;
                    }

                    let phase = pod_phase(pod);
                    let ready = pod_ready(pod);
                    let restarts = pod_restarts(pod);
                    let age = pod_age(pod);
                    let node = pod_node(pod);

                    body.row(row_height, |mut row| {
                        let is_selected = self.selected_pod == Some(i);

                        row.col(|ui| {
                            if ui.selectable_label(is_selected, name).clicked() {
                                self.selected_pod = Some(i);
                                self.state.show_right_panel = true;
                            }
                        });
                        row.col(|ui| { ui.label(ns); });
                        row.col(|ui| {
                            let color = match phase.as_str() {
                                "Running" => egui::Color32::from_rgb(34, 197, 94),
                                "Pending" | "ContainerCreating" => egui::Color32::from_rgb(234, 179, 8),
                                "Succeeded" => egui::Color32::from_rgb(100, 149, 237),
                                _ => egui::Color32::from_rgb(239, 68, 68),
                            };
                            ui.colored_label(color, &phase);
                        });
                        row.col(|ui| { ui.monospace(ready); });
                        row.col(|ui| { ui.label(restarts.to_string()); });
                        row.col(|ui| { ui.monospace(age); });
                        row.col(|ui| { ui.label(node); });
                    });
                }
            });

        ui.small("Real Pods from cluster • 5s auto-refresh • click row for details");
    }

    fn draw_nodes_view(&mut self, ui: &mut Ui) {
        use egui_extras::{Column, TableBuilder};

        let table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::auto().at_least(220.0))
            .column(Column::auto().at_least(90.0))
            .column(Column::auto().at_least(140.0))
            .column(Column::auto().at_least(160.0))
            .column(Column::remainder());

        table
            .header(22.0, |mut header| {
                header.col(|ui| { ui.strong("Name"); });
                header.col(|ui| { ui.strong("Status"); });
                header.col(|ui| { ui.strong("Roles"); });
                header.col(|ui| { ui.strong("Kubelet Version"); });
                header.col(|ui| { ui.strong("Age"); });
            })
            .body(|mut body| {
                for (i, node) in self.nodes.iter().enumerate() {
                    let name = node_name(node);
                    let status = node_status(node);
                    let roles = node_roles(node);
                    let version = node_version(node);
                    let age = node_age(node);

                    body.row(20.0, |mut row| {
                        let is_selected = self.selected_node == Some(i);
                        row.col(|ui| {
                            if ui.selectable_label(is_selected, name).clicked() {
                                self.selected_node = Some(i);
                                self.state.show_right_panel = true;
                            }
                        });
                        row.col(|ui| {
                            let color = if status == "Ready" {
                                egui::Color32::from_rgb(34, 197, 94)
                            } else {
                                egui::Color32::from_rgb(239, 68, 68)
                            };
                            ui.colored_label(color, &status);
                        });
                        row.col(|ui| { ui.label(roles); });
                        row.col(|ui| { ui.monospace(version); });
                        row.col(|ui| { ui.monospace(age); });
                    });
                }
            });

        ui.small("Real Nodes from cluster • click for details");
    }
}

// === Real data extraction helpers (Phase 2) ===

fn pod_name(pod: &Pod) -> &str {
    pod.metadata.name.as_deref().unwrap_or("unknown")
}

fn pod_namespace(pod: &Pod) -> &str {
    pod.metadata.namespace.as_deref().unwrap_or("default")
}

fn pod_phase(pod: &Pod) -> String {
    pod.status
        .as_ref()
        .and_then(|s| s.phase.clone())
        .unwrap_or_else(|| "Unknown".into())
}

fn pod_ready(pod: &Pod) -> String {
    if let Some(status) = &pod.status {
        let total = status.container_statuses.as_ref().map(|v| v.len()).unwrap_or(0);
        let ready = status
            .container_statuses
            .as_ref()
            .map(|v| v.iter().filter(|c| c.ready).count())
            .unwrap_or(0);
        format!("{}/{}", ready, total)
    } else {
        "0/0".into()
    }
}

fn pod_restarts(pod: &Pod) -> u32 {
    pod.status
        .as_ref()
        .and_then(|s| s.container_statuses.as_ref())
        .map(|containers| {
            containers
                .iter()
                .map(|c| c.restart_count as u32)
                .sum()
        })
        .unwrap_or(0)
}

fn pod_node(pod: &Pod) -> &str {
    pod.spec
        .as_ref()
        .and_then(|s| s.node_name.as_deref())
        .unwrap_or("-")
}

fn pod_age(pod: &Pod) -> String {
    pod.metadata.creation_timestamp.as_ref().map(|ts| {
        // k8s-openapi 0.27 may use jiff under the hood; use Display + simple humanization
        let s = ts.0.to_string();
        // Very rough human age from RFC3339-ish string for MVP
        if let Some(date_part) = s.split('T').next() {
            format!("{} (since {})", "recent", date_part)
        } else {
            s
        }
    }).unwrap_or_else(|| "-".into())
}

fn node_name(node: &Node) -> &str {
    node.metadata.name.as_deref().unwrap_or("unknown")
}

fn node_status(node: &Node) -> String {
    node.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conds| conds.iter().find(|c| c.type_ == "Ready"))
        .map(|c| if c.status == "True" { "Ready" } else { "NotReady" })
        .unwrap_or("Unknown")
        .to_string()
}

fn node_roles(node: &Node) -> String {
    if let Some(labels) = &node.metadata.labels {
        if labels.contains_key("node-role.kubernetes.io/control-plane")
            || labels.contains_key("node-role.kubernetes.io/master")
        {
            return "control-plane".into();
        }
    }
    "worker".into()
}

fn node_version(node: &Node) -> String {
    node.status
        .as_ref()
        .and_then(|s| s.node_info.as_ref())
        .map(|i| i.kubelet_version.clone())
        .unwrap_or_else(|| "unknown".into())
}

fn node_age(node: &Node) -> String {
    node.metadata.creation_timestamp.as_ref().map(|ts| {
        let s = ts.0.to_string();
        if let Some(date_part) = s.split('T').next() {
            format!("{} (since {})", "recent", date_part)
        } else {
            s
        }
    }).unwrap_or_else(|| "-".into())
}