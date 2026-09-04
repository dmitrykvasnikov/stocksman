import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type WheelEvent,
} from "react";

import type { Candle } from "./marketData";

interface CandlestickChartProps {
  candles: Candle[];
  signalsVisible?: boolean;
  initialViewport?: ChartViewport;
  onViewportChange?: (viewport: ChartViewport) => void;
}

export interface ChartViewport {
  visibleCandleCount: number | null;
  startTimestamp: number | null;
  followLatest: boolean;
}

interface DragState {
  pointerId: number;
  startIndex: number;
  startX: number;
}

const WIDTH = 1_200;
const HEIGHT = 620;
const PLOT_LEFT = 26;
const PLOT_RIGHT = 94;
const PRICE_TOP = 24;
const PRICE_HEIGHT = 396;
const VOLUME_TOP = 464;
const VOLUME_HEIGHT = 100;
const PRICE_TICKS = 5;
const MIN_VISIBLE_CANDLES = 10;
const ZOOM_FACTOR = 0.75;
const PAN_RATIO = 0.2;

function formatPrice(value: number): string {
  const fractionDigits = value >= 1_000 ? 0 : value >= 10 ? 2 : 4;
  return new Intl.NumberFormat(undefined, {
    minimumFractionDigits: fractionDigits,
    maximumFractionDigits: fractionDigits,
  }).format(value);
}

function formatVolume(value: number): string {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(value);
}

function formatTime(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(timestamp);
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum);
}

export default function CandlestickChart({
  candles,
  signalsVisible = true,
  initialViewport,
  onViewportChange,
}: CandlestickChartProps) {
  const [requestedVisibleCount, setRequestedVisibleCount] = useState<number | null>(
    initialViewport?.visibleCandleCount ?? null,
  );
  const [requestedStartTimestamp, setRequestedStartTimestamp] = useState<number | null>(
    initialViewport?.startTimestamp ?? null,
  );
  const [isFollowingLatest, setIsFollowingLatest] = useState(
    initialViewport?.followLatest ?? true,
  );
  const [inspectedTimestamp, setInspectedTimestamp] = useState<number | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const dragState = useRef<DragState | null>(null);

  const minimumVisibleCount = Math.min(MIN_VISIBLE_CANDLES, candles.length);
  const visibleCount = clamp(
    requestedVisibleCount ?? candles.length,
    minimumVisibleCount,
    candles.length,
  );
  const maximumStartIndex = candles.length - visibleCount;
  const matchingStartIndex =
    requestedStartTimestamp === null
      ? 0
      : candles.findIndex((candle) => candle.timestamp >= requestedStartTimestamp);
  const requestedStartIndex = matchingStartIndex === -1 ? maximumStartIndex : matchingStartIndex;
  const startIndex = isFollowingLatest
    ? maximumStartIndex
    : clamp(requestedStartIndex, 0, maximumStartIndex);

  useEffect(() => {
    onViewportChange?.({
      visibleCandleCount: requestedVisibleCount,
      startTimestamp: isFollowingLatest ? null : (candles[startIndex]?.timestamp ?? null),
      followLatest: isFollowingLatest,
    });
  }, [
    candles,
    isFollowingLatest,
    onViewportChange,
    requestedVisibleCount,
    startIndex,
  ]);

  if (candles.length === 0) {
    return null;
  }

  const visibleCandles = candles.slice(startIndex, startIndex + visibleCount);
  const inspectedCandle =
    visibleCandles.find((candle) => candle.timestamp === inspectedTimestamp) ??
    visibleCandles.at(-1)!;

  const lows = visibleCandles.map((candle) => candle.low);
  const highs = visibleCandles.map((candle) => candle.high);
  const rawMinimum = Math.min(...lows);
  const rawMaximum = Math.max(...highs);
  const rawRange = Math.max(rawMaximum - rawMinimum, rawMaximum * 0.001);
  const priceMinimum = rawMinimum - rawRange * 0.08;
  const priceMaximum = rawMaximum + rawRange * 0.08;
  const priceRange = priceMaximum - priceMinimum;
  const maximumVolume = Math.max(...visibleCandles.map((candle) => candle.volume), 1);
  const plotWidth = WIDTH - PLOT_LEFT - PLOT_RIGHT;
  const candleStep = plotWidth / visibleCandles.length;
  const bodyWidth = Math.max(2, Math.min(11, candleStep * 0.62));
  const priceY = (price: number) =>
    PRICE_TOP + ((priceMaximum - price) / priceRange) * PRICE_HEIGHT;
  const timeTickIndexes = Array.from(
    new Set([0, Math.floor((visibleCandles.length - 1) / 2), visibleCandles.length - 1]),
  );
  const inspectedVisibleIndex = visibleCandles.findIndex(
    (candle) => candle.timestamp === inspectedCandle.timestamp,
  );
  const inspectedX = PLOT_LEFT + inspectedVisibleIndex * candleStep + candleStep / 2;
  const isAtDefaultViewport = visibleCount === candles.length && startIndex === 0;

  const setViewportStart = (nextStartIndex: number) => {
    setIsFollowingLatest(false);
    setRequestedStartTimestamp(
      candles[clamp(nextStartIndex, 0, maximumStartIndex)].timestamp,
    );
  };

  const zoom = (direction: "in" | "out", anchorRatio = 0.5) => {
    const nextVisibleCount =
      direction === "in"
        ? Math.max(minimumVisibleCount, Math.floor(visibleCount * ZOOM_FACTOR))
        : Math.min(candles.length, Math.ceil(visibleCount / ZOOM_FACTOR));

    if (nextVisibleCount === visibleCount) {
      return;
    }

    const anchoredIndex = startIndex + visibleCount * anchorRatio;
    const nextStartIndex = Math.round(anchoredIndex - nextVisibleCount * anchorRatio);
    setIsFollowingLatest(false);
    setRequestedVisibleCount(nextVisibleCount);
    setRequestedStartTimestamp(
      candles[clamp(nextStartIndex, 0, candles.length - nextVisibleCount)].timestamp,
    );
    setInspectedTimestamp(null);
  };

  const resetViewport = () => {
    setRequestedVisibleCount(null);
    setRequestedStartTimestamp(null);
    setIsFollowingLatest(true);
    setInspectedTimestamp(null);
  };

  const followLatest = () => {
    setRequestedStartTimestamp(null);
    setIsFollowingLatest(true);
    setInspectedTimestamp(null);
  };

  const panByPage = (direction: -1 | 1) => {
    const candleDelta = Math.max(1, Math.round(visibleCount * PAN_RATIO));
    setViewportStart(startIndex + direction * candleDelta);
    setInspectedTimestamp(null);
  };

  const eventAnchorRatio = (clientX: number, element: SVGSVGElement): number => {
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0) {
      return 0.5;
    }
    const plotLeft = bounds.left + (PLOT_LEFT / WIDTH) * bounds.width;
    const renderedPlotWidth = (plotWidth / WIDTH) * bounds.width;
    return clamp((clientX - plotLeft) / renderedPlotWidth, 0, 1);
  };

  const inspectAtPointer = (event: PointerEvent<SVGSVGElement>) => {
    const ratio = eventAnchorRatio(event.clientX, event.currentTarget);
    const visibleIndex = clamp(
      Math.floor(ratio * visibleCandles.length),
      0,
      visibleCandles.length - 1,
    );
    setInspectedTimestamp(visibleCandles[visibleIndex].timestamp);
  };

  const handlePointerDown = (event: PointerEvent<SVGSVGElement>) => {
    if (event.button !== 0 || maximumStartIndex === 0) {
      return;
    }
    dragState.current = {
      pointerId: event.pointerId,
      startIndex,
      startX: event.clientX,
    };
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setIsDragging(true);
  };

  const handlePointerMove = (event: PointerEvent<SVGSVGElement>) => {
    const drag = dragState.current;
    if (!drag) {
      inspectAtPointer(event);
      return;
    }

    const bounds = event.currentTarget.getBoundingClientRect();
    const renderedPlotWidth = (plotWidth / WIDTH) * bounds.width;
    if (renderedPlotWidth <= 0) {
      return;
    }
    const pixelsPerCandle = renderedPlotWidth / visibleCount;
    const candleDelta = Math.round((drag.startX - event.clientX) / pixelsPerCandle);
    setViewportStart(drag.startIndex + candleDelta);
  };

  const finishPointerDrag = (event: PointerEvent<SVGSVGElement>) => {
    if (dragState.current?.pointerId !== event.pointerId) {
      return;
    }
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    dragState.current = null;
    setIsDragging(false);
    inspectAtPointer(event);
  };

  const handleWheel = (event: WheelEvent<SVGSVGElement>) => {
    event.preventDefault();
    zoom(event.deltaY < 0 ? "in" : "out", eventAnchorRatio(event.clientX, event.currentTarget));
  };

  const handleKeyDown = (event: KeyboardEvent<SVGSVGElement>) => {
    switch (event.key) {
      case "ArrowLeft":
        event.preventDefault();
        setViewportStart(startIndex - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        setViewportStart(startIndex + 1);
        break;
      case "+":
      case "=":
        event.preventDefault();
        zoom("in");
        break;
      case "-":
      case "_":
        event.preventDefault();
        zoom("out");
        break;
      case "0":
        event.preventDefault();
        resetViewport();
        break;
    }
  };

  return (
    <div
      className="chart-frame"
      data-signal-overlays={signalsVisible ? "visible" : "hidden"}
    >
      <div className="chart-toolbar">
        <div className="chart-readout" aria-label="Inspected candle values" aria-live="polite">
          <span>{formatTime(inspectedCandle.timestamp)}</span>
          <span>
            O <strong>{formatPrice(inspectedCandle.open)}</strong>
          </span>
          <span>
            H <strong>{formatPrice(inspectedCandle.high)}</strong>
          </span>
          <span>
            L <strong>{formatPrice(inspectedCandle.low)}</strong>
          </span>
          <span>
            C <strong>{formatPrice(inspectedCandle.close)}</strong>
          </span>
          <span>
            Vol <strong>{formatVolume(inspectedCandle.volume)}</strong>
          </span>
        </div>
        <div className="chart-viewport-controls" role="group" aria-label="Chart viewport">
          <span className="chart-viewport-status" aria-live="polite">
            {startIndex + 1}–{startIndex + visibleCount} of {candles.length}
          </span>
          <button
            type="button"
            aria-label="Pan left"
            disabled={startIndex === 0}
            onClick={() => panByPage(-1)}
          >
            ←
          </button>
          <button
            type="button"
            aria-label="Pan right"
            disabled={startIndex === maximumStartIndex}
            onClick={() => panByPage(1)}
          >
            →
          </button>
          <button
            type="button"
            aria-label="Zoom out"
            disabled={visibleCount === candles.length}
            onClick={() => zoom("out")}
          >
            −
          </button>
          <button
            type="button"
            aria-label="Zoom in"
            disabled={visibleCount === minimumVisibleCount}
            onClick={() => zoom("in")}
          >
            +
          </button>
          <button type="button" disabled={isAtDefaultViewport} onClick={resetViewport}>
            Reset
          </button>
          <button
            className="chart-follow-latest"
            type="button"
            role="switch"
            aria-checked={isFollowingLatest}
            aria-label="Follow latest candle"
            onClick={() => {
              if (isFollowingLatest) {
                setIsFollowingLatest(false);
                setRequestedStartTimestamp(candles[startIndex].timestamp);
              } else {
                followLatest();
              }
            }}
          >
            Latest
          </button>
        </div>
      </div>

      <svg
        className={`candlestick-chart${isDragging ? " candlestick-chart--dragging" : ""}`}
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        tabIndex={0}
        aria-label={`${visibleCandles.length} candle price and volume chart for ${inspectedCandle.symbol}`}
        aria-description="Drag or use the arrow keys to pan. Scroll or use plus and minus to zoom. Move the pointer over the chart to inspect OHLCV values."
        onKeyDown={handleKeyDown}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={finishPointerDrag}
        onPointerCancel={finishPointerDrag}
        onPointerLeave={() => {
          if (!dragState.current) {
            setInspectedTimestamp(null);
          }
        }}
        onWheel={handleWheel}
      >
        <rect className="chart-surface" width={WIDTH} height={HEIGHT} />

        {Array.from({ length: PRICE_TICKS }, (_, index) => {
          const ratio = index / (PRICE_TICKS - 1);
          const y = PRICE_TOP + PRICE_HEIGHT * ratio;
          const price = priceMaximum - priceRange * ratio;
          return (
            <g key={`price-${index}`}>
              <line
                className="chart-grid-line"
                x1={PLOT_LEFT}
                x2={WIDTH - PLOT_RIGHT}
                y1={y}
                y2={y}
              />
              <text className="chart-axis-label" x={WIDTH - PLOT_RIGHT + 14} y={y + 4}>
                {formatPrice(price)}
              </text>
            </g>
          );
        })}

        <line
          className="chart-divider"
          x1={PLOT_LEFT}
          x2={WIDTH - PLOT_RIGHT}
          y1={VOLUME_TOP - 18}
          y2={VOLUME_TOP - 18}
        />
        <text className="chart-pane-label" x={PLOT_LEFT} y={VOLUME_TOP - 26}>
          VOLUME
        </text>

        {visibleCandles.map((candle, index) => {
          const x = PLOT_LEFT + index * candleStep + candleStep / 2;
          const openY = priceY(candle.open);
          const closeY = priceY(candle.close);
          const rising = candle.close >= candle.open;
          const bodyTop = Math.min(openY, closeY);
          const bodyHeight = Math.max(1.8, Math.abs(closeY - openY));
          const volumeHeight = (candle.volume / maximumVolume) * VOLUME_HEIGHT;
          const direction = rising ? "up" : "down";

          return (
            <g
              className={`chart-candle chart-candle--${direction}`}
              key={candle.timestamp}
              onPointerEnter={() => setInspectedTimestamp(candle.timestamp)}
            >
              <title>{`${formatTime(candle.timestamp)} · O ${formatPrice(candle.open)} · H ${formatPrice(candle.high)} · L ${formatPrice(candle.low)} · C ${formatPrice(candle.close)} · Vol ${formatVolume(candle.volume)}`}</title>
              <line
                className="candle-wick"
                x1={x}
                x2={x}
                y1={priceY(candle.high)}
                y2={priceY(candle.low)}
              />
              <rect
                className="candle-body"
                x={x - bodyWidth / 2}
                y={bodyTop}
                width={bodyWidth}
                height={bodyHeight}
                rx={0.8}
              />
              <rect
                className="volume-bar"
                x={x - bodyWidth / 2}
                y={VOLUME_TOP + VOLUME_HEIGHT - volumeHeight}
                width={bodyWidth}
                height={volumeHeight}
                rx={0.8}
              />
            </g>
          );
        })}

        <g className="chart-crosshair" aria-hidden="true">
          <line
            x1={inspectedX}
            x2={inspectedX}
            y1={PRICE_TOP}
            y2={VOLUME_TOP + VOLUME_HEIGHT}
          />
          <line
            x1={PLOT_LEFT}
            x2={WIDTH - PLOT_RIGHT}
            y1={priceY(inspectedCandle.close)}
            y2={priceY(inspectedCandle.close)}
          />
          <circle cx={inspectedX} cy={priceY(inspectedCandle.close)} r={3.5} />
        </g>

        {timeTickIndexes.map((index) => {
          const x = PLOT_LEFT + index * candleStep + candleStep / 2;
          const anchor =
            index === 0 ? "start" : index === visibleCandles.length - 1 ? "end" : "middle";
          return (
            <text
              className="chart-axis-label"
              key={`time-${visibleCandles[index].timestamp}`}
              x={x}
              y={HEIGHT - 20}
              textAnchor={anchor}
            >
              {formatTime(visibleCandles[index].timestamp)}
            </text>
          );
        })}
      </svg>
      <p className="chart-interaction-hint">
        Drag to pan · Scroll to zoom · Hover to inspect · Press 0 to reset
      </p>
    </div>
  );
}
