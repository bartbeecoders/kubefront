//! Embedded interactive terminal.
//!
//! A real cross-platform pseudo-terminal (via `portable-pty`) hosts the user's
//! login shell; the React side renders it with xterm.js. Output bytes are pushed
//! over a Tauri [`Channel`]; keystrokes and resize events come back through the
//! `terminal_write` / `terminal_resize` commands.
//!
//! kubectl scoping: when the active connection is Direct (a local kubeconfig), we
//! materialise a *temporary* kubeconfig whose `current-context` is pinned to the
//! context the app currently has selected, and point `KUBECONFIG` at it. This is
//! deliberate — the app's in-memory context can differ from the file's default,
//! and a terminal silently targeting the wrong cluster (e.g. prod) would be
//! dangerous. The temp file is removed when the terminal closes. Remote
//! connections have no local kubeconfig, so kubectl is left unconfigured and the
//! banner says so.
//!
//! This module is OS plumbing only (process + PTY); no Kubernetes client logic
//! lives here, keeping the `kube-core` boundary intact.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::SharedBackend;
use crate::conn::Active;

/// Output / lifecycle events streamed to the frontend xterm instance.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "lowercase")]
pub enum TerminalEvent {
    /// Raw bytes from the PTY (xterm writes them verbatim).
    Output(Vec<u8>),
    /// The shell exited; carries its exit code when known.
    Exit(Option<i32>),
}

/// A live terminal session held in the [`Backend`](crate::commands::Backend).
/// Dropping it (on close) tears down the writer/master; the reader thread then
/// sees EOF and emits [`TerminalEvent::Exit`].
pub struct TerminalSession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// Temp kubeconfig to delete on close (Direct connections only).
    temp_kubeconfig: Option<PathBuf>,
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.killer.kill();
        if let Some(p) = &self.temp_kubeconfig {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Write a copy of `src` kubeconfig with `current-context` pinned to `context`,
/// into a private temp file. Returns its path. Best-effort `0600` on unix.
fn scoped_kubeconfig(id: u64, src: &Path, context: Option<&str>) -> Option<PathBuf> {
    let mut kc = kube::config::Kubeconfig::read_from(src).ok()?;
    if let Some(ctx) = context {
        kc.current_context = Some(ctx.to_string());
    }
    let yaml = serde_yaml::to_string(&kc).ok()?;
    let path = std::env::temp_dir().join(format!("kubefront-term-{id}.yaml"));
    std::fs::write(&path, yaml).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Some(path)
}

/// The login shell to spawn and its interactive args.
fn shell_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        let prog = std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".into());
        CommandBuilder::new(prog)
    }
    #[cfg(not(windows))]
    {
        let prog = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let mut cmd = CommandBuilder::new(prog);
        // Interactive so we get a prompt + job control.
        cmd.arg("-i");
        cmd
    }
}

/// Open a new terminal. `on_event` receives PTY output; the returned id keys the
/// session for `terminal_write` / `terminal_resize` / `terminal_close`.
#[tauri::command]
pub async fn terminal_open(
    state: State<'_, SharedBackend>,
    on_event: Channel<TerminalEvent>,
    cols: u16,
    rows: u16,
) -> Result<u64, String> {
    // Snapshot what we need about the active connection, then drop the lock for
    // the (synchronous but quick) PTY setup.
    let (id, kubeconfig_path, context, is_remote) = {
        let mut b = state.lock().await;
        let id = b.next_terminal_id;
        b.next_terminal_id += 1;
        let is_remote = matches!(b.active, Some(Active::Remote(_)));
        (
            id,
            b.manager.path.clone(),
            b.manager.current_context.clone(),
            is_remote,
        )
    };

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("open pty: {e}"))?;

    let mut cmd = shell_command();
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = dirs_home() {
        cmd.cwd(home);
    }

    // Scope kubectl to the active Direct cluster (see module docs).
    let temp_kubeconfig = match (is_remote, kubeconfig_path.as_ref()) {
        (false, Some(path)) => {
            let tmp = scoped_kubeconfig(id, path, context.as_deref());
            if let Some(p) = &tmp {
                cmd.env("KUBECONFIG", p);
            }
            tmp
        }
        _ => None,
    };

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn shell: {e}"))?;
    drop(pair.slave);

    let killer = child.clone_killer();
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("pty reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("pty writer: {e}"))?;

    // Greeting banner (kube context / shell hint), sent before shell output.
    let _ = on_event.send(TerminalEvent::Output(banner(is_remote, context.as_deref())));

    // Blocking read loop on its own thread → pushes output to the channel, then
    // reaps the child for an exit code on EOF.
    std::thread::spawn(move || {
        let mut child = child;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if on_event
                        .send(TerminalEvent::Output(buf[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        let code = child.wait().ok().map(|s| s.exit_code() as i32);
        let _ = on_event.send(TerminalEvent::Exit(code));
    });

    let mut b = state.lock().await;
    b.terminals.insert(
        id,
        TerminalSession {
            writer,
            master: pair.master,
            killer,
            temp_kubeconfig,
        },
    );
    tracing::info!("Opened terminal {id} (remote={is_remote})");
    Ok(id)
}

/// Send keystrokes (xterm `onData`) to the shell's stdin.
#[tauri::command]
pub async fn terminal_write(
    state: State<'_, SharedBackend>,
    id: u64,
    data: String,
) -> Result<(), String> {
    let mut b = state.lock().await;
    let session = b
        .terminals
        .get_mut(&id)
        .ok_or_else(|| "terminal not found".to_string())?;
    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

/// Resize the PTY when the xterm viewport changes.
#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, SharedBackend>,
    id: u64,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let b = state.lock().await;
    let session = b
        .terminals
        .get(&id)
        .ok_or_else(|| "terminal not found".to_string())?;
    session
        .master
        .resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
}

/// Close a terminal: kill the shell, drop the PTY, delete the temp kubeconfig.
#[tauri::command]
pub async fn terminal_close(state: State<'_, SharedBackend>, id: u64) -> Result<(), String> {
    let mut b = state.lock().await;
    // Drop runs the teardown (kill + temp-file cleanup).
    b.terminals.remove(&id);
    tracing::info!("Closed terminal {id}");
    Ok(())
}

/// The user's home directory (PTY working dir), if discoverable.
fn dirs_home() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().to_path_buf())
}

/// One-line greeting written to the terminal before the shell prompt.
fn banner(is_remote: bool, context: Option<&str>) -> Vec<u8> {
    // ANSI: dim cyan title.
    let body = if is_remote {
        "KubeFront terminal — remote connection: kubectl is not preconfigured here.".to_string()
    } else if let Some(ctx) = context {
        format!("KubeFront terminal — kubectl scoped to context \"{ctx}\".")
    } else {
        "KubeFront terminal — no active cluster; kubectl uses your default kubeconfig.".to_string()
    };
    format!("\x1b[2;36m{body}\x1b[0m\r\n").into_bytes()
}
