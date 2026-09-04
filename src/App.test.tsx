import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  invokeMock.mockReset();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("presents the read-only product boundary in a browser preview", async () => {
    render(<App />);

    expect(
      screen.getByRole("heading", {
        name: /btc \/ usdt/i,
      }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Browser preview")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /btc \/ usdt 1h/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText(/open the desktop app to load binance spot data/i)).toBeInTheDocument();
    expect(screen.getByText(/no accounts · no api keys · no trading/i)).toBeInTheDocument();
  });

  it("loads Binance controls and keeps multiple desktop chart tabs independent", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    invokeMock.mockResolvedValue({
      application: "Stocksman",
      runtime: "Rust + Tokio",
      backend: {
        state: "ready",
        endpoint: "http://127.0.0.1:49152",
      },
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const url = new URL(input instanceof Request ? input.url : input.toString());
      if (url.pathname === "/market-data/catalog") {
        return Promise.resolve(
          Response.json({
            instruments: ["BTC", "ETH", "BNB", "SOL", "XRP"].map((base_asset) => ({
              provider: "binance",
              symbol: `${base_asset}USDT`,
              base_asset,
              quote_asset: "USDT",
            })),
            intervals: [
              { id: "1s", label: "1 second", amount: 1, unit: "second" },
              { id: "1m", label: "1 minute", amount: 1, unit: "minute" },
              { id: "3m", label: "3 minutes", amount: 3, unit: "minute" },
              { id: "5m", label: "5 minutes", amount: 5, unit: "minute" },
              { id: "15m", label: "15 minutes", amount: 15, unit: "minute" },
              { id: "30m", label: "30 minutes", amount: 30, unit: "minute" },
              { id: "1h", label: "1 hour", amount: 1, unit: "hour" },
              { id: "2h", label: "2 hours", amount: 2, unit: "hour" },
              { id: "4h", label: "4 hours", amount: 4, unit: "hour" },
              { id: "6h", label: "6 hours", amount: 6, unit: "hour" },
              { id: "8h", label: "8 hours", amount: 8, unit: "hour" },
              { id: "12h", label: "12 hours", amount: 12, unit: "hour" },
              { id: "1d", label: "1 day", amount: 1, unit: "day" },
              { id: "3d", label: "3 days", amount: 3, unit: "day" },
              { id: "1w", label: "1 week", amount: 1, unit: "week" },
              { id: "1M", label: "1 month", amount: 1, unit: "month" },
            ],
          }),
        );
      }

      const symbol = url.searchParams.get("symbol") ?? "BTCUSDT";
      const interval = url.searchParams.get("interval") ?? "1h";
      return Promise.resolve(
        Response.json({
          candles: [
            {
              provider: "binance",
              symbol,
              interval,
              timestamp: 1_704_492_000_000,
              open: 42_000,
              high: 42_300,
              low: 41_900,
              close: 42_200,
              volume: 75,
              closed: true,
            },
            {
              provider: "binance",
              symbol,
              interval,
              timestamp: 1_704_495_600_000,
              open: 42_200,
              high: 42_250,
              low: 41_800,
              close: 41_950,
              volume: 62,
              closed: false,
            },
          ],
        }),
      );
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    expect(await screen.findByText("Binance feed ready")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: /2 candle price and volume chart for btcusdt/i }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("runtime_info");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.objectContaining({
        href: expect.stringMatching(/\/market-data\/candles\?.*provider=binance/),
      }),
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );

    expect(await screen.findAllByRole("option", { name: /\/ USDT$/ })).toHaveLength(5);
    expect(screen.getByRole("option", { name: "XRP / USDT" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "1 second" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "1 month" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Timeframe" }).children).toHaveLength(16);
    expect(screen.getByRole("switch", { name: "Signal overlays" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.change(screen.getByRole("combobox", { name: "Symbol" }), {
      target: { value: "ETHUSDT" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "Timeframe" }), {
      target: { value: "5m" },
    });

    expect(
      await screen.findByRole("heading", { name: "ETH / USDT" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("ETHUSDT · Binance Spot · 5 minutes")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: /2 candle price and volume chart for ethusdt/i }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: "Signal overlays" }));
    expect(screen.getByRole("switch", { name: "Signal overlays" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(document.querySelector(".chart-frame")).toHaveAttribute(
      "data-signal-overlays",
      "hidden",
    );
    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        expect.objectContaining({
          href: expect.stringMatching(/symbol=ETHUSDT.*interval=5m/),
        }),
        expect.objectContaining({ signal: expect.any(AbortSignal) }),
      );
    });

    const firstTab = screen.getByRole("tab", { name: /eth \/ usdt 5m/i });
    fireEvent.click(screen.getByRole("button", { name: "Add chart tab" }));

    expect(screen.getAllByRole("tab")).toHaveLength(2);
    expect(firstTab).toHaveAttribute("aria-selected", "false");
    expect(screen.getAllByRole("tab", { name: /eth \/ usdt 5m/i })[1]).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("switch", { name: "Signal overlays" })).toHaveAttribute(
      "aria-checked",
      "false",
    );

    fireEvent.click(screen.getByRole("switch", { name: "Signal overlays" }));

    fireEvent.change(screen.getByRole("combobox", { name: "Symbol" }), {
      target: { value: "SOLUSDT" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "Timeframe" }), {
      target: { value: "1d" },
    });

    expect(await screen.findByRole("heading", { name: "SOL / USDT" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /sol \/ usdt 1d/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    fireEvent.click(firstTab);
    expect(screen.getByRole("combobox", { name: "Symbol" })).toHaveValue("ETHUSDT");
    expect(screen.getByRole("combobox", { name: "Timeframe" })).toHaveValue("5m");
    expect(screen.getByRole("switch", { name: "Signal overlays" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    expect(firstTab).toHaveAttribute("aria-selected", "true");

    fireEvent.click(screen.getByRole("button", { name: /close eth \/ usdt 5m chart tab/i }));
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    expect(screen.getByRole("tab", { name: /sol \/ usdt 1d/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("combobox", { name: "Symbol" })).toHaveValue("SOLUSDT");
  });
});
