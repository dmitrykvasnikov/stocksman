import {
  CandlestickSeries,
  HistogramSeries,
  createChart,
  createSeriesMarkers,
  type IChartApi,
  type IPrimitivePaneRenderer,
  type ISeriesPrimitive,
  type SeriesMarker,
  type Time,
} from "lightweight-charts";
import chartPackage from "lightweight-charts/package.json";
import { describe, expect, it } from "vitest";

const marker: SeriesMarker<Time> = {
  time: 1_704_067_200 as Time,
  position: "aboveBar",
  color: "#e4b860",
  shape: "circle",
  text: "Signal",
};

const spanRenderer: IPrimitivePaneRenderer = {
  draw: () => undefined,
  drawBackground: () => undefined,
};

const spanPrimitive: ISeriesPrimitive = {
  paneViews: () => [
    {
      zOrder: () => "bottom",
      renderer: () => spanRenderer,
    },
  ],
};

function exerciseRequiredChartApi(chart: IChartApi) {
  const candleSeries = chart.addSeries(CandlestickSeries);
  chart.addSeries(HistogramSeries, { priceFormat: { type: "volume" } }, 1);
  const markers = createSeriesMarkers(candleSeries, [marker]);

  candleSeries.attachPrimitive(spanPrimitive);
  chart.subscribeCrosshairMove(() => undefined);
  chart.timeScale().subscribeVisibleLogicalRangeChange(() => undefined);
  chart.applyOptions({ handleScroll: true, handleScale: true });

  return markers;
}

describe("Lightweight Charts selection", () => {
  it("pins the verified permissive version", () => {
    expect(chartPackage.version).toBe("5.2.0");
    expect(chartPackage.license).toBe("Apache-2.0");
  });

  it("exposes the rendering and extension APIs required by the workbench", () => {
    expect(CandlestickSeries.type).toBe("Candlestick");
    expect(HistogramSeries.type).toBe("Histogram");
    expect(createChart).toBeTypeOf("function");
    expect(createSeriesMarkers).toBeTypeOf("function");
    expect(exerciseRequiredChartApi).toBeTypeOf("function");
    expect(spanPrimitive.paneViews?.()[0]?.renderer()).toBe(spanRenderer);
  });
});
