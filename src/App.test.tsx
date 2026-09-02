import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

afterEach(() => {
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
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            candles: [
              {
                provider: "mock",
                symbol: "BTCUSDT",
                interval: "1h",
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
                symbol: "BTCUSDT",
                interval: "1h",
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
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      ),
    );

    render(<App />);

    expect(await screen.findByText("Mock feed ready")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: /2 candle price and volume chart for btcusdt/i }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("runtime_info");
    expect(fetch).toHaveBeenCalledWith(
      expect.objectContaining({
        href: expect.stringContaining("/market-data/candles?"),
      }),
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
  });
});
