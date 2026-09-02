import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

type BackendState = "reconnecting" | "ready" | "unavailable";
type RuntimeState = BackendState | "browser";

interface RuntimeInfo {
  application: string;
  runtime: string;
  backend: {
    state: BackendState;
    endpoint: string | null;
  };
}

const statusCopy: Record<RuntimeState, string> = {
  reconnecting: "Backend reconnecting…",
  ready: "Backend ready",
  browser: "Browser preview",
  unavailable: "Backend unavailable",
};

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [runtimeState, setRuntimeState] = useState<RuntimeState>(() =>
    isTauriRuntime() ? "reconnecting" : "browser",
  );
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfo | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let active = true;

    const refreshBackendState = () => {
      void invoke<RuntimeInfo>("runtime_info")
        .then((info) => {
          if (active) {
            setRuntimeInfo(info);
            setRuntimeState(info.backend.state);
          }
        })
        .catch(() => {
          if (active) {
            setRuntimeState("unavailable");
          }
        });
    };

    refreshBackendState();
    const refreshTimer = window.setInterval(refreshBackendState, 1_000);

    return () => {
      active = false;
      window.clearInterval(refreshTimer);
    };
  }, []);

  return (
    <main className="shell">
      <header className="topbar">
        <a className="brand" href="#top" aria-label="Stocksman home">
          <span className="brand-mark" aria-hidden="true">
            S
          </span>
          <span>Stocksman</span>
        </a>
        <span className={`runtime-status runtime-status--${runtimeState}`}>
          <span className="status-dot" aria-hidden="true" />
          {statusCopy[runtimeState]}
        </span>
      </header>

      <section className="hero" id="top">
        <p className="eyebrow">Read-only market workbench</p>
        <h1>Crypto charts, signals, and nothing that can move your money.</h1>
        <p className="summary">
          The desktop foundation is running. Market replay, candlesticks, and signal
          tools will arrive as small, testable slices.
        </p>

        <div className="foundation-card" aria-label="Foundation status">
          <div>
            <span className="card-label">Application</span>
            <strong>{runtimeInfo?.application ?? "Stocksman"}</strong>
          </div>
          <div>
            <span className="card-label">Desktop</span>
            <strong>Tauri 2</strong>
          </div>
          <div>
            <span className="card-label">Backend</span>
            <strong>{runtimeInfo?.runtime ?? "Rust + Tokio"}</strong>
          </div>
        </div>
      </section>

      <footer>
        No accounts · No API keys · No trading
      </footer>
    </main>
  );
}
