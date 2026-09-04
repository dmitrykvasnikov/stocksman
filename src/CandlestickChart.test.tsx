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
    expect(screen.getByRole("button", { name: "Pan left" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Follow latest candle" })).toHaveAttribute(
      "aria-checked",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));

    const chart = screen.getByRole("img", {
      name: /30 candle price and volume chart for btcusdt/i,
    });
    expect(screen.getByText("6–35 of 40")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeEnabled();
    expect(screen.getByRole("switch", { name: "Follow latest candle" })).toHaveAttribute(
      "aria-checked",
      "false",
    );

    fireEvent.click(screen.getByRole("button", { name: "Pan right" }));
    expect(screen.getByText("11–40 of 40")).toBeInTheDocument();

    fireEvent.keyDown(chart, { key: "ArrowLeft" });
    expect(screen.getByText("10–39 of 40")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(screen.getByText("1–40 of 40")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
  });

  it("preserves a manual viewport across updates and follows new latest candles on demand", () => {
    const initialCandles = Array.from({ length: 40 }, (_, index) => candle(index));
    const { rerender } = render(<CandlestickChart candles={initialCandles} />);

    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("6–35 of 40")).toBeInTheDocument();

    rerender(
      <CandlestickChart candles={Array.from({ length: 41 }, (_, index) => candle(index))} />,
    );
    expect(screen.getByText("6–35 of 41")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("switch", { name: "Follow latest candle" }));
    expect(screen.getByText("12–41 of 41")).toBeInTheDocument();

    rerender(
      <CandlestickChart candles={Array.from({ length: 42 }, (_, index) => candle(index))} />,
    );
    expect(screen.getByText("13–42 of 42")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("switch", { name: "Follow latest candle" }));
    rerender(
      <CandlestickChart candles={Array.from({ length: 43 }, (_, index) => candle(index))} />,
    );
    expect(screen.getByText("13–42 of 43")).toBeInTheDocument();
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
