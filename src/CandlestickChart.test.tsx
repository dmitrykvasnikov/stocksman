import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import CandlestickChart from "./CandlestickChart";
import type { Candle } from "./marketData";

afterEach(cleanup);

function candle(index: number): Candle {
  const open = 100 + index;
  const close = open + (index % 2 === 0 ? 2 : -1);
  return {
    provider: "mock",
    symbol: "BTCUSDT",
    interval: "1h",
    timestamp: 1_704_067_200_000 + index * 3_600_000,
    open,
    high: Math.max(open, close) + 3,
    low: Math.min(open, close) - 2,
    close,
    volume: 1_000 + index * 10,
    closed: true,
  };
}

describe("CandlestickChart", () => {
  it("zooms, pans with the keyboard, and resets the viewport", () => {
    const candles = Array.from({ length: 40 }, (_, index) => candle(index));
    render(<CandlestickChart candles={candles} />);

    expect(screen.getByText("1–40 of 40")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));

    const chart = screen.getByRole("img", {
      name: /30 candle price and volume chart for btcusdt/i,
    });
    expect(screen.getByText("6–35 of 40")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeEnabled();

    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(screen.getByText("5–34 of 40")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(screen.getByText("1–40 of 40")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
  });

  it("updates the OHLCV readout for the inspected candle", () => {
    const candles = Array.from({ length: 12 }, (_, index) => candle(index));
    const { container } = render(<CandlestickChart candles={candles} />);
    const readout = screen.getByLabelText("Inspected candle values");

    expect(readout).toHaveTextContent("O 111.00");
    expect(readout).toHaveTextContent("Vol 1,110");

    const firstCandle = container.querySelector(".chart-candle");
    expect(firstCandle).not.toBeNull();
    fireEvent.pointerEnter(firstCandle!);

    expect(readout).toHaveTextContent("O 100.00");
    expect(readout).toHaveTextContent("H 105.00");
    expect(readout).toHaveTextContent("L 98.00");
    expect(readout).toHaveTextContent("C 102.00");
    expect(readout).toHaveTextContent("Vol 1,000");
  });
});
