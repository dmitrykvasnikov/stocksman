import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import CandlestickChart from "./CandlestickChart";
import { loadCandleHistory, type Candle } from "./marketData";

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
  ready: "Mock feed ready",
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
  const [candles, setCandles] = useState<Candle[]>([]);
  const [chartError, setChartError] = useState(false);

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

  const backendEndpoint = runtimeInfo?.backend.endpoint;
  useEffect(() => {
    if (!backendEndpoint) {
      return;
    }

    const controller = new AbortController();
    void loadCandleHistory(
      backendEndpoint,
      {
        provider: "mock",
        symbol: "BTCUSDT",
        interval: "1h",
        start_timestamp: null,
        end_timestamp: null,
        limit: 80,
      },
      controller.signal,
    )
      .then((history) => {
        setCandles(history.candles);
        setChartError(false);
      })
      .catch((error: unknown) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setChartError(true);
        }
      });

    return () => controller.abort();
  }, [backendEndpoint]);

  return (
    <main className="shell">
      <header className="topbar" id="top">
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

      <section className="workbench" aria-labelledby="chart-title">
        <div className="workbench-heading">
          <div>
            <p className="eyebrow">Read-only market workbench</p>
            <h1 id="chart-title">BTC / USDT</h1>
            <p className="market-meta">Bitcoin · Mock replay · 1 hour</p>
          </div>
          <div className="feed-badge">
            <span>OFFLINE DATA</span>
            <strong>Deterministic replay</strong>
          </div>
        </div>

        <div className="chart-card">
          {candles.length > 0 ? (
            <CandlestickChart candles={candles} />
          ) : (
            <div className="chart-empty" role="status">
              <div className="chart-empty-grid" aria-hidden="true" />
              <strong>
                {chartError
                  ? "Candle history is unavailable"
                  : runtimeState === "browser"
                    ? "Open the desktop app to load the offline replay"
                    : "Loading deterministic candles…"}
              </strong>
              <span>
                {chartError
                  ? "The chart will retry when the local backend reconnects."
                  : "No Binance connection or credentials are required."}
              </span>
            </div>
          )}
        </div>
      </section>

      <footer>No accounts · No API keys · No trading</footer>
    </main>
  );
}
