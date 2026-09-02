mod backend;
mod supervisor;

use serde::Serialize;
use supervisor::{BackendStatus, BackendSupervisor};
use tauri::Manager;

#[derive(Debug, PartialEq, Serialize)]
struct RuntimeInfo {
    application: &'static str,
    runtime: &'static str,
    backend: BackendStatus,
}

fn current_runtime_info(supervisor: &BackendSupervisor) -> RuntimeInfo {
    RuntimeInfo {
        application: "Stocksman",
        runtime: "Rust + Tokio",
        backend: supervisor.status(),
    }
}

#[tauri::command]
fn runtime_info(supervisor: tauri::State<'_, BackendSupervisor>) -> RuntimeInfo {
    current_runtime_info(&supervisor)
}

pub fn run_backend_sidecar() -> std::io::Result<()> {
    backend::run()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let application = tauri::Builder::default()
        .setup(|application| {
            application.manage(BackendSupervisor::start());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![runtime_info])
        .build(tauri::generate_context!())
        .expect("failed to build Stocksman desktop shell");

    application.run(|handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            handle.state::<BackendSupervisor>().shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::BackendState;

    #[test]
    fn reports_ready_from_the_tokio_runtime() {
        let supervisor = BackendSupervisor::with_status(BackendStatus {
            state: BackendState::Ready,
            endpoint: Some("http://127.0.0.1:1234".to_owned()),
        });

        assert_eq!(
            current_runtime_info(&supervisor),
            RuntimeInfo {
                application: "Stocksman",
                runtime: "Rust + Tokio",
                backend: BackendStatus {
                    state: BackendState::Ready,
                    endpoint: Some("http://127.0.0.1:1234".to_owned()),
                },
            }
        );
    }
}
