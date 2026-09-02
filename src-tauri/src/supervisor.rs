use std::{
    process::Stdio,
    sync::{Arc, RwLock},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::watch,
    time::{sleep, timeout},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const RESTART_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendState {
    Reconnecting,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BackendStatus {
    pub state: BackendState,
    pub endpoint: Option<String>,
}

#[derive(Clone)]
pub struct BackendSupervisor {
    status: Arc<RwLock<BackendStatus>>,
    shutdown: watch::Sender<bool>,
}

#[derive(Deserialize)]
struct BackendAnnouncement {
    port: u16,
}

impl BackendSupervisor {
    pub fn start() -> Self {
        let status = Arc::new(RwLock::new(BackendStatus {
            state: BackendState::Reconnecting,
            endpoint: None,
        }));
        let (shutdown, shutdown_rx) = watch::channel(false);

        tauri::async_runtime::spawn(supervise(status.clone(), shutdown_rx));

        Self { status, shutdown }
    }

    pub fn status(&self) -> BackendStatus {
        self.status.read().expect("backend status lock").clone()
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    #[cfg(test)]
    pub fn with_status(status: BackendStatus) -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            status: Arc::new(RwLock::new(status)),
            shutdown,
        }
    }
}

async fn supervise(status: Arc<RwLock<BackendStatus>>, mut shutdown: watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow() {
            return;
        }

        update_status(&status, BackendState::Reconnecting, None);

        match start_backend().await {
            Ok((mut child, announcement)) => {
                update_status(
                    &status,
                    BackendState::Ready,
                    Some(format!("http://127.0.0.1:{}", announcement.port)),
                );

                tokio::select! {
                    _ = child.wait() => {}
                    changed = shutdown.changed() => {
                        if changed.is_ok() && *shutdown.borrow() {
                            stop_child(&mut child).await;
                            return;
                        }
                    }
                }

                update_status(&status, BackendState::Reconnecting, None);
            }
            Err(error) => {
                eprintln!("backend unavailable: {error}");
                update_status(&status, BackendState::Unavailable, None);
            }
        }

        tokio::select! {
            _ = sleep(RESTART_DELAY) => {}
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    return;
                }
            }
        }
    }
}

async fn start_backend() -> Result<(Child, BackendAnnouncement), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate application executable: {error}"))?;
    let mut child = Command::new(executable)
        .arg("--stocksman-backend")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("could not start backend process: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "backend stdout was not captured".to_owned())?;
    let mut lines = BufReader::new(stdout).lines();

    let announcement = timeout(STARTUP_TIMEOUT, lines.next_line())
        .await
        .map_err(|_| "backend did not report ready before the startup timeout".to_owned())?
        .map_err(|error| format!("could not read backend readiness: {error}"))?
        .ok_or_else(|| "backend exited before reporting ready".to_owned())?;

    match serde_json::from_str(&announcement) {
        Ok(announcement) => {
            tauri::async_runtime::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("backend: {line}");
                }
            });
            Ok((child, announcement))
        }
        Err(error) => {
            stop_child(&mut child).await;
            Err(format!("backend reported invalid readiness data: {error}"))
        }
    }
}

async fn stop_child(child: &mut Child) {
    if let Err(error) = child.kill().await {
        eprintln!("could not stop backend process: {error}");
    }
}

fn update_status(status: &RwLock<BackendStatus>, state: BackendState, endpoint: Option<String>) {
    *status.write().expect("backend status lock") = BackendStatus { state, endpoint };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_transitions_clear_stale_endpoints() {
        let status = RwLock::new(BackendStatus {
            state: BackendState::Ready,
            endpoint: Some("http://127.0.0.1:1234".to_owned()),
        });

        update_status(&status, BackendState::Reconnecting, None);

        assert_eq!(
            status.into_inner().expect("backend status lock"),
            BackendStatus {
                state: BackendState::Reconnecting,
                endpoint: None,
            }
        );
    }
}
