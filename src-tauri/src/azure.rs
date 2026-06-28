//! Azure CLI orchestration for adding AKS clusters.
//!
//! This module shells out to the `az` (and `kubelogin`) command-line tools — it is
//! pure OS/CLI orchestration, NOT Kubernetes client logic, so per AGENTS.md it
//! lives in `src-tauri` rather than `kube-core`. The only kube-rs touch is reading
//! back a context name from the freshly written kubeconfig via the shared
//! [`KubeConfigManager`].
//!
//! Auth model: we fetch AAD (Azure AD) credentials with `az aks get-credentials`
//! (no `--admin`) and then run `kubelogin convert-kubeconfig -l azurecli` so the
//! kubeconfig's `exec` stanza mints tokens from the existing `az login` session.
//! kube-rs supports the `exec` credential plugin but NOT the legacy in-tree
//! `azure` auth-provider, which is exactly why the conversion step is required.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use kubefront_core::KubeConfigManager;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ============================================================================
// DTOs sent to the frontend (snake_case mirrors `src/types.ts`).
// ============================================================================

/// Result of probing the local Azure CLI for an active login.
#[derive(Debug, Clone, Serialize, Default)]
pub struct AzureStatus {
    /// Whether the `az` binary could be found and run.
    pub installed: bool,
    /// Whether `az account show` succeeded (an active `az login` session exists).
    pub logged_in: bool,
    /// Signed-in user (UPN / service-principal id), when available.
    pub user: Option<String>,
    /// Active tenant id, when available.
    pub tenant_id: Option<String>,
    /// Human-readable reason when not installed / not logged in.
    pub error: Option<String>,
}

/// One Azure subscription visible to the signed-in account.
#[derive(Debug, Clone, Serialize)]
pub struct AzureSubscription {
    pub id: String,
    pub name: String,
    pub tenant_id: Option<String>,
    pub is_default: bool,
}

/// One AKS managed cluster within a subscription.
#[derive(Debug, Clone, Serialize)]
pub struct AksCluster {
    pub name: String,
    pub resource_group: String,
    pub location: Option<String>,
    pub kubernetes_version: Option<String>,
    /// Power state code (e.g. "Running" / "Stopped"), when reported.
    pub power_state: Option<String>,
    /// Whether the cluster has Azure AD integration enabled.
    pub aad_enabled: bool,
}

// ============================================================================
// Public API
// ============================================================================

/// Probe the Azure CLI: is it installed, and is there an active `az login`?
/// Never errors — every failure mode is encoded in the returned [`AzureStatus`].
pub async fn status() -> AzureStatus {
    match run("az", &["account", "show", "-o", "json"]).await {
        Ok(stdout) => match serde_json::from_str::<AccountShow>(&stdout) {
            Ok(acc) => AzureStatus {
                installed: true,
                logged_in: true,
                user: acc.user.and_then(|u| u.name),
                tenant_id: acc.tenant_id,
                error: None,
            },
            Err(e) => AzureStatus {
                installed: true,
                logged_in: false,
                error: Some(format!("Could not parse `az account show` output: {e}")),
                ..Default::default()
            },
        },
        Err(e) if e.missing => AzureStatus {
            installed: false,
            error: Some(
                "Azure CLI (`az`) was not found on PATH. Install it from \
                 https://aka.ms/azure-cli, then restart KubeFront."
                    .into(),
            ),
            ..Default::default()
        },
        Err(e) => AzureStatus {
            installed: true,
            logged_in: false,
            error: Some(format!(
                "Not logged in to Azure. Run `az login`, then retry. ({})",
                e.message
            )),
            ..Default::default()
        },
    }
}

/// List the subscriptions the signed-in account can see.
pub async fn subscriptions() -> Result<Vec<AzureSubscription>, String> {
    let stdout = run("az", &["account", "list", "-o", "json"])
        .await
        .map_err(|e| e.az_message())?;
    let raw: Vec<RawSubscription> =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse subscriptions: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| AzureSubscription {
            id: r.id,
            name: r.name,
            tenant_id: r.tenant_id,
            is_default: r.is_default.unwrap_or(false),
        })
        .collect())
}

/// List the AKS clusters within a subscription.
pub async fn aks_clusters(subscription_id: &str) -> Result<Vec<AksCluster>, String> {
    let stdout = run(
        "az",
        &[
            "aks",
            "list",
            "--subscription",
            subscription_id,
            "-o",
            "json",
        ],
    )
    .await
    .map_err(|e| e.az_message())?;
    let raw: Vec<RawAks> =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse AKS clusters: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| AksCluster {
            name: r.name,
            resource_group: r.resource_group,
            location: r.location,
            kubernetes_version: r.current_kubernetes_version.or(r.kubernetes_version),
            power_state: r.power_state.and_then(|p| p.code),
            aad_enabled: r.aad_profile.is_some(),
        })
        .collect())
}

/// Fetch AAD credentials for an AKS cluster, convert the kubeconfig to use the
/// `azurecli` exec login, and return `(kubeconfig_path, context_name)`. The caller
/// registers the path as a Direct connection.
pub async fn add_aks(
    subscription_id: &str,
    resource_group: &str,
    cluster_name: &str,
) -> Result<(PathBuf, Option<String>), String> {
    let path = aks_kubeconfig_path(resource_group, cluster_name)?;
    let path_str = path.to_string_lossy().into_owned();

    run(
        "az",
        &[
            "aks",
            "get-credentials",
            "--subscription",
            subscription_id,
            "-g",
            resource_group,
            "-n",
            cluster_name,
            "--file",
            &path_str,
            "--overwrite-existing",
        ],
    )
    .await
    .map_err(|e| e.az_message())?;

    run(
        "kubelogin",
        &[
            "convert-kubeconfig",
            "-l",
            "azurecli",
            "--kubeconfig",
            &path_str,
        ],
    )
    .await
    .map_err(|e| e.kubelogin_message())?;

    let context = read_context(&path);
    Ok((path, context))
}

// ============================================================================
// Internals
// ============================================================================

/// Where a per-cluster kubeconfig is written: an `aks/` subfolder of the same
/// config dir that holds `settings.json`. One file per cluster keeps the Direct
/// entry id (the path) stable across re-imports.
fn aks_kubeconfig_path(resource_group: &str, cluster_name: &str) -> Result<PathBuf, String> {
    let proj = directories::ProjectDirs::from("dev", "kube-front", "KubeFront")
        .ok_or_else(|| "Could not resolve the KubeFront config directory".to_string())?;
    let dir = proj.config_dir().join("aks");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let file = format!(
        "{}_{}.kubeconfig",
        sanitize(resource_group),
        sanitize(cluster_name)
    );
    Ok(dir.join(file))
}

/// Reduce a name to filesystem-safe characters for the kubeconfig filename.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Read the (single) context name from a freshly written kubeconfig, via the
/// shared manager so the kube-rs touch stays in one place.
fn read_context(path: &Path) -> Option<String> {
    let mut mgr = KubeConfigManager::new();
    mgr.load_from_path(path).ok()?;
    mgr.current_context
        .clone()
        .or_else(|| mgr.contexts.first().map(|c| c.name.clone()))
}

/// An error from running an external CLI, with a flag for "binary not found" so
/// callers can give an install hint rather than a raw error.
struct AzError {
    message: String,
    missing: bool,
}

impl AzError {
    /// Message for `az` failures — adds an install hint when `az` is missing.
    fn az_message(&self) -> String {
        if self.missing {
            "Azure CLI (`az`) was not found on PATH. Install it from \
             https://aka.ms/azure-cli, then restart KubeFront."
                .into()
        } else {
            self.message.clone()
        }
    }

    /// Message for `kubelogin` failures — adds an install hint when it is missing.
    fn kubelogin_message(&self) -> String {
        if self.missing {
            "`kubelogin` was not found on PATH. Install it (e.g. `az aks install-cli`) \
             so KubeFront can use your `az login` session for Azure AD auth, then retry."
                .into()
        } else {
            format!("kubelogin failed: {}", self.message)
        }
    }
}

/// Run a CLI program, capturing stdout. On Windows the call is routed through
/// `cmd /C` (so batch-file shims like `az.cmd` resolve) with no console window.
async fn run(program: &str, args: &[&str]) -> Result<String, AzError> {
    let output = match build(program, args).output().await {
        Ok(o) => o,
        Err(e) => {
            return Err(AzError {
                missing: e.kind() == std::io::ErrorKind::NotFound,
                message: format!("failed to run {program}: {e}"),
            })
        }
    };
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let missing = looks_missing(&stderr);
        let message = first_meaningful_line(&stderr)
            .unwrap_or_else(|| format!("{program} exited with status {:?}", output.status.code()));
        Err(AzError { missing, message })
    }
}

/// Build the (tokio) command, wrapping in `cmd /C` on Windows so `.cmd` shims
/// resolve and no console window flashes.
fn build(program: &str, args: &[&str]) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = StdCommand::new("cmd");
        c.arg("/C").arg(program).args(args);
        c.creation_flags(CREATE_NO_WINDOW);
        Command::from(c)
    }
    #[cfg(not(windows))]
    {
        let mut c = StdCommand::new(program);
        c.args(args);
        Command::from(c)
    }
}

/// Heuristic: does this stderr indicate the program itself was not found?
/// (On Windows the `cmd /C` wrapper always spawns, so a missing `az`/`kubelogin`
/// surfaces here rather than as a spawn error.)
fn looks_missing(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("is not recognized")
        || s.contains("not recognized as")
        || s.contains("command not found")
        || s.contains("no such file")
}

/// First non-empty line of CLI stderr, for a concise error surface.
fn first_meaningful_line(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

// ============================================================================
// Raw `az` JSON shapes (camelCase) — mapped into the snake_case frontend DTOs.
// ============================================================================

#[derive(Deserialize)]
struct AccountShow {
    user: Option<AccountUser>,
    #[serde(rename = "tenantId")]
    tenant_id: Option<String>,
}

#[derive(Deserialize)]
struct AccountUser {
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawSubscription {
    id: String,
    name: String,
    #[serde(rename = "tenantId")]
    tenant_id: Option<String>,
    #[serde(rename = "isDefault")]
    is_default: Option<bool>,
}

#[derive(Deserialize)]
struct RawAks {
    name: String,
    #[serde(rename = "resourceGroup")]
    resource_group: String,
    location: Option<String>,
    #[serde(rename = "kubernetesVersion")]
    kubernetes_version: Option<String>,
    #[serde(rename = "currentKubernetesVersion")]
    current_kubernetes_version: Option<String>,
    #[serde(rename = "powerState")]
    power_state: Option<PowerState>,
    #[serde(rename = "aadProfile")]
    aad_profile: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct PowerState {
    code: Option<String>,
}
