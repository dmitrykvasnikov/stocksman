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
    expect(screen.getByText(/open the desktop app to load the offline replay/i)).toBeInTheDocument();
    expect(screen.getByText(/no accounts · no api keys · no trading/i)).toBeInTheDocument();
  });

  it("reports the supervised backend state in the desktop runtime", async () => {
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
            instruments: [
              {
                provider: "mock",
                symbol: "BTCUSDT",
                base_asset: "BTC",
                quote_asset: "USDT",
              },
              {
                provider: "mock",
                symbol: "ETHUSDT",
                base_asset: "ETH",
                quote_asset: "USDT",
              },
            ],
            intervals: [
              { id: "5m", label: "5 minutes", amount: 5, unit: "minute" },
              { id: "1h", label: "1 hour", amount: 1, unit: "hour" },
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
              provider: "mock",
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
              provider: "mock",
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

    expect(await screen.findByText("Mock feed ready")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: /2 candle price and volume chart for btcusdt/i }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("runtime_info");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.objectContaining({
        href: expect.stringContaining("/market-data/candles?"),
      }),
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );

    expect(await screen.findByRole("option", { name: "ETH / USDT" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Symbol" }), {
      target: { value: "ETHUSDT" },
    });
    fireEvent.change(screen.getByRole("combobox", { name: "Timeframe" }), {
      target: { value: "5m" },
    });

    expect(
      await screen.findByRole("heading", { name: "ETH / USDT" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("ETHUSDT · Mock replay · 5 minutes")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: /2 candle price and volume chart for ethusdt/i }),
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        expect.objectContaining({
          href: expect.stringMatching(/symbol=ETHUSDT.*interval=5m/),
        }),
        expect.objectContaining({ signal: expect.any(AbortSignal) }),
      );
    });
  });
});
