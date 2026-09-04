import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

import CandlestickChart, { type ChartViewport } from "./CandlestickChart";
import {
  loadUserConfiguration,
  saveUserConfiguration,
  type UserConfiguration,
  type WorkspaceConfiguration,
} from "./configuration";
import {
  loadCandleHistory,
  loadMarketDataCatalog,
  type Candle,
  type MarketDataCatalog,
} from "./marketData";

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

interface ChartTab {
  id: number;
  provider: string;
  symbol: string;
  interval: string;
  signalsVisible: boolean;
  viewport: ChartViewport;
}

interface WorkspaceState {
  tabs: ChartTab[];
  activeTabId: number;
}

const statusCopy: Record<RuntimeState, string> = {
  reconnecting: "Backend reconnecting…",
  ready: "Binance feed ready",
  browser: "Browser preview",
  unavailable: "Backend unavailable",
};

const PROVIDER = "binance";
const DEFAULT_SYMBOL = "BTCUSDT";
const DEFAULT_INTERVAL = "1h";
const MAX_CHART_TABS = 12;

const defaultViewport: ChartViewport = {
  visibleCandleCount: null,
  startTimestamp: null,
  followLatest: true,
};

const defaultWorkspace: WorkspaceState = {
  tabs: [
    {
      id: 1,
      provider: PROVIDER,
      symbol: DEFAULT_SYMBOL,
      interval: DEFAULT_INTERVAL,
      signalsVisible: true,
      viewport: defaultViewport,
    },
  ],
  activeTabId: 1,
};

function fromConfiguration(workspace: WorkspaceConfiguration): WorkspaceState {
  return {
    tabs: workspace.tabs.map((tab) => ({
      id: tab.id,
      provider: tab.provider,
      symbol: tab.symbol,
      interval: tab.interval,
      signalsVisible: tab.signals_visible,
      viewport: {
        visibleCandleCount: tab.viewport.visible_candle_count,
        startTimestamp: tab.viewport.start_timestamp,
        followLatest: tab.viewport.follow_latest,
      },
    })),
    activeTabId: workspace.active_tab_id,
  };
}

function toConfiguration(workspace: WorkspaceState): WorkspaceConfiguration {
  return {
    tabs: workspace.tabs.map((tab) => ({
      id: tab.id,
      provider: tab.provider,
      symbol: tab.symbol,
      interval: tab.interval,
      signals_visible: tab.signalsVisible,
      viewport: {
        visible_candle_count: tab.viewport.visibleCandleCount,
        start_timestamp: tab.viewport.startTimestamp,
        follow_latest: tab.viewport.followLatest,
      },
    })),
    active_tab_id: workspace.activeTabId,
  };
}

const fallbackCatalog: MarketDataCatalog = {
  instruments: [
    {
      provider: PROVIDER,
      symbol: DEFAULT_SYMBOL,
      base_asset: "BTC",
      quote_asset: "USDT",
    },
  ],
  intervals: [{ id: DEFAULT_INTERVAL, label: "1 hour", amount: 1, unit: "hour" }],
};

interface ChartTabPanelProps {
  active: boolean;
  backendEndpoint: string | null | undefined;
  runtimeState: RuntimeState;
  tab: ChartTab;
  onViewportChange: (tabId: number, viewport: ChartViewport) => void;
}

function ChartTabPanel({
  active,
  backendEndpoint,
  runtimeState,
  tab,
  onViewportChange,
}: ChartTabPanelProps) {
  const requestKey = `${backendEndpoint ?? "offline"}:${tab.provider}:${tab.symbol}:${tab.interval}`;
  const [loadState, setLoadState] = useState<{
    requestKey: string;
    candles: Candle[];
    chartError: boolean;
  }>({ requestKey: "", candles: [], chartError: false });
  const candles = loadState.requestKey === requestKey ? loadState.candles : [];
  const chartError = loadState.requestKey === requestKey && loadState.chartError;

  useEffect(() => {
    if (!backendEndpoint) {
      return;
    }

    let mounted = true;
    const controller = new AbortController();
    void loadCandleHistory(
      backendEndpoint,
      {
        provider: tab.provider,
        symbol: tab.symbol,
        interval: tab.interval,
        start_timestamp: null,
        end_timestamp: null,
        limit: 80,
      },
      controller.signal,
    )
      .then((history) => {
        if (mounted) {
          setLoadState({ requestKey, candles: history.candles, chartError: false });
        }
      })
      .catch((error: unknown) => {
        if (mounted && !(error instanceof DOMException && error.name === "AbortError")) {
          setLoadState({ requestKey, candles: [], chartError: true });
        }
      });

    return () => {
      mounted = false;
      controller.abort();
    };
  }, [backendEndpoint, requestKey, tab.interval, tab.provider, tab.symbol]);

  return (
    <div
      id={`chart-panel-${tab.id}`}
      role="tabpanel"
      aria-labelledby={`chart-tab-${tab.id}`}
      hidden={!active}
    >
      <div className="chart-card">
        {candles.length > 0 ? (
          <CandlestickChart
            key={`${tab.symbol}-${tab.interval}`}
            candles={candles}
            initialViewport={tab.viewport}
            onViewportChange={(viewport) => onViewportChange(tab.id, viewport)}
            signalsVisible={tab.signalsVisible}
          />
        ) : (
          <div className="chart-empty" role="status">
            <div className="chart-empty-grid" aria-hidden="true" />
            <strong>
              {chartError
                ? "Candle history is unavailable"
                : runtimeState === "browser"
                  ? "Open the desktop app to load Binance Spot data"
                  : "Loading Binance candles…"}
            </strong>
            <span>
              {chartError
                ? "The chart will retry when the local backend reconnects."
                : "No Binance account or credentials are required."}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [runtimeState, setRuntimeState] = useState<RuntimeState>(() =>
    isTauriRuntime() ? "reconnecting" : "browser",
  );
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfo | null>(null);
  const [catalog, setCatalog] = useState<MarketDataCatalog>(fallbackCatalog);
  const [workspace, setWorkspace] = useState<WorkspaceState>(defaultWorkspace);
  const [userConfiguration, setUserConfiguration] = useState<UserConfiguration | null>(null);
  const [configurationEndpoint, setConfigurationEndpoint] = useState<string | null>(null);
  const nextTabId = useRef(2);
  const { tabs, activeTabId } = workspace;

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
    void loadMarketDataCatalog(backendEndpoint, PROVIDER, controller.signal)
      .then(setCatalog)
      .catch(() => undefined);

    return () => controller.abort();
  }, [backendEndpoint]);

  useEffect(() => {
    if (!backendEndpoint) {
      return;
    }

    let active = true;
    const controller = new AbortController();
    void loadUserConfiguration(backendEndpoint, controller.signal)
      .then((configuration) => {
        if (!active) {
          return;
        }
        const restoredWorkspace = fromConfiguration(configuration.workspace);
        setWorkspace(restoredWorkspace);
        setUserConfiguration(configuration);
        setConfigurationEndpoint(backendEndpoint);
        nextTabId.current = Math.max(...restoredWorkspace.tabs.map((tab) => tab.id)) + 1;
      })
      .catch(() => undefined);

    return () => {
      active = false;
      controller.abort();
    };
  }, [backendEndpoint]);

  useEffect(() => {
    if (
      !backendEndpoint ||
      configurationEndpoint !== backendEndpoint ||
      userConfiguration === null
    ) {
      return;
    }

    const controller = new AbortController();
    const saveTimer = window.setTimeout(() => {
      void saveUserConfiguration(
        backendEndpoint,
        { ...userConfiguration, workspace: toConfiguration(workspace) },
        controller.signal,
      ).catch(() => undefined);
    }, 250);

    return () => {
      window.clearTimeout(saveTimer);
      controller.abort();
    };
  }, [backendEndpoint, configurationEndpoint, userConfiguration, workspace]);

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0];

  const updateActiveTab = (updates: Partial<Omit<ChartTab, "id">>) => {
    setWorkspace((currentWorkspace) => ({
      ...currentWorkspace,
      tabs: currentWorkspace.tabs.map((tab) =>
        tab.id === currentWorkspace.activeTabId ? { ...tab, ...updates } : tab,
      ),
    }));
  };

  const updateTabViewport = useCallback((tabId: number, viewport: ChartViewport) => {
    setWorkspace((currentWorkspace) => {
      const tab = currentWorkspace.tabs.find((candidate) => candidate.id === tabId);
      if (
        !tab ||
        (tab.viewport.visibleCandleCount === viewport.visibleCandleCount &&
          tab.viewport.startTimestamp === viewport.startTimestamp &&
          tab.viewport.followLatest === viewport.followLatest)
      ) {
        return currentWorkspace;
      }
      return {
        ...currentWorkspace,
        tabs: currentWorkspace.tabs.map((candidate) =>
          candidate.id === tabId ? { ...candidate, viewport } : candidate,
        ),
      };
    });
  }, []);

  const addTab = () => {
    if (tabs.length >= MAX_CHART_TABS) {
      return;
    }
    const newTab = { ...activeTab, id: nextTabId.current };
    nextTabId.current += 1;
    setWorkspace((currentWorkspace) => ({
      tabs: [...currentWorkspace.tabs, newTab],
      activeTabId: newTab.id,
    }));
  };

  const closeTab = (tabId: number) => {
    if (tabs.length === 1) {
      return;
    }

    const closingIndex = tabs.findIndex((tab) => tab.id === tabId);
    const nextActiveTab = tabs[closingIndex + 1] ?? tabs[closingIndex - 1];
    setWorkspace((currentWorkspace) => ({
      tabs: currentWorkspace.tabs.filter((tab) => tab.id !== tabId),
      activeTabId: tabId === activeTabId ? nextActiveTab.id : activeTabId,
    }));
  };

  const selectTabByIndex = (nextIndex: number) => {
    const nextTab = tabs[nextIndex];
    setWorkspace((currentWorkspace) => ({
      ...currentWorkspace,
      activeTabId: nextTab.id,
    }));
    window.requestAnimationFrame(() => {
      document.getElementById(`chart-tab-${nextTab.id}`)?.focus();
    });
  };

  const selectedInstrument =
    catalog.instruments.find((instrument) => instrument.symbol === activeTab.symbol) ??
    fallbackCatalog.instruments[0];
  const selectedInterval =
    catalog.intervals.find((definition) => definition.id === activeTab.interval) ??
    fallbackCatalog.intervals[0];

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
        <div className="chart-tabs">
          <div className="chart-tab-list" role="tablist" aria-label="Chart tabs">
            {tabs.map((tab, index) => {
              const instrument =
                catalog.instruments.find((item) => item.symbol === tab.symbol) ??
                fallbackCatalog.instruments[0];
              const isActive = tab.id === activeTab.id;

              return (
                <div className="chart-tab-item" key={tab.id}>
                  <button
                    id={`chart-tab-${tab.id}`}
                    className="chart-tab"
                    type="button"
                    role="tab"
                    aria-selected={isActive}
                    aria-controls={`chart-panel-${tab.id}`}
                    tabIndex={isActive ? 0 : -1}
                    onClick={() =>
                      setWorkspace((currentWorkspace) => ({
                        ...currentWorkspace,
                        activeTabId: tab.id,
                      }))
                    }
                    onKeyDown={(event) => {
                      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                        event.preventDefault();
                        const direction = event.key === "ArrowLeft" ? -1 : 1;
                        selectTabByIndex((index + direction + tabs.length) % tabs.length);
                      } else if (event.key === "Home" || event.key === "End") {
                        event.preventDefault();
                        selectTabByIndex(event.key === "Home" ? 0 : tabs.length - 1);
                      }
                    }}
                  >
                    <span>
                      {instrument.base_asset} / {instrument.quote_asset}
                    </span>
                    <small>{tab.interval}</small>
                  </button>
                  {tabs.length > 1 ? (
                    <button
                      className="chart-tab-close"
                      type="button"
                      aria-label={`Close ${instrument.base_asset} / ${instrument.quote_asset} ${tab.interval} chart tab`}
                      onClick={() => closeTab(tab.id)}
                    >
                      <span aria-hidden="true">×</span>
                    </button>
                  ) : null}
                </div>
              );
            })}
          </div>
          <button
            className="chart-tab-add"
            type="button"
            aria-label="Add chart tab"
            disabled={tabs.length >= MAX_CHART_TABS}
            onClick={addTab}
          >
            <span aria-hidden="true">+</span>
          </button>
        </div>

        <div className="workbench-heading">
          <div>
            <p className="eyebrow">Read-only market workbench</p>
            <h1 id="chart-title">
              {selectedInstrument.base_asset} / {selectedInstrument.quote_asset}
            </h1>
            <p className="market-meta">
              {selectedInstrument.symbol} · Binance Spot · {selectedInterval.label}
            </p>
          </div>
          <div className="chart-controls" aria-label="Chart configuration">
            <label>
              <span>Symbol</span>
              <select
                value={activeTab.symbol}
                onChange={(event) =>
                  updateActiveTab({ symbol: event.target.value, viewport: defaultViewport })
                }
              >
                {catalog.instruments.map((instrument) => (
                  <option key={instrument.symbol} value={instrument.symbol}>
                    {instrument.base_asset} / {instrument.quote_asset}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Timeframe</span>
              <select
                value={activeTab.interval}
                onChange={(event) =>
                  updateActiveTab({ interval: event.target.value, viewport: defaultViewport })
                }
              >
                {catalog.intervals.map((definition) => (
                  <option key={definition.id} value={definition.id}>
                    {definition.label}
                  </option>
                ))}
              </select>
            </label>
            <div className="signal-visibility-control">
              <span>Signals</span>
              <button
                className="signal-visibility-switch"
                type="button"
                role="switch"
                aria-checked={activeTab.signalsVisible}
                aria-label="Signal overlays"
                title="Show or hide this tab's configured signal overlays"
                onClick={() =>
                  updateActiveTab({ signalsVisible: !activeTab.signalsVisible })
                }
              >
                <span className="signal-visibility-switch__track" aria-hidden="true">
                  <span />
                </span>
                {activeTab.signalsVisible ? "Visible" : "Hidden"}
              </button>
            </div>
            <div className="feed-badge">
              <span>PUBLIC DATA</span>
              <strong>Read-only Binance Spot</strong>
            </div>
          </div>
        </div>

        {tabs.map((tab) => (
          <ChartTabPanel
            key={tab.id}
            active={tab.id === activeTab.id}
            backendEndpoint={backendEndpoint}
            runtimeState={runtimeState}
            tab={tab}
            onViewportChange={updateTabViewport}
          />
        ))}
      </section>

      <footer>No accounts · No API keys · No trading</footer>
    </main>
  );
}
