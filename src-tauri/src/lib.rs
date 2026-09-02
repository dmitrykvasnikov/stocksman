use serde::Serialize;

#[derive(Debug, PartialEq, Serialize)]
struct RuntimeInfo {
    application: &'static str,
    runtime: &'static str,
    state: &'static str,
}

async fn current_runtime_info() -> RuntimeInfo {
    tokio::task::yield_now().await;

    RuntimeInfo {
        application: "Stocksman",
        runtime: "Rust + Tokio",
        state: "ready",
    }
}

#[tauri::command]
async fn runtime_info() -> RuntimeInfo {
    current_runtime_info().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![runtime_info])
        .run(tauri::generate_context!())
        .expect("failed to run Stocksman desktop shell");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_ready_from_the_tokio_runtime() {
        assert_eq!(
            current_runtime_info().await,
            RuntimeInfo {
                application: "Stocksman",
                runtime: "Rust + Tokio",
                state: "ready",
            }
        );
    }
}
