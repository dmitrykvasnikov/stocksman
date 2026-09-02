import type { Candle } from "./marketData";

interface CandlestickChartProps {
  candles: Candle[];
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

export default function CandlestickChart({ candles }: CandlestickChartProps) {
  if (candles.length === 0) {
    return null;
  }

  const lows = candles.map((candle) => candle.low);
  const highs = candles.map((candle) => candle.high);
  const rawMinimum = Math.min(...lows);
  const rawMaximum = Math.max(...highs);
  const rawRange = Math.max(rawMaximum - rawMinimum, rawMaximum * 0.001);
  const priceMinimum = rawMinimum - rawRange * 0.08;
  const priceMaximum = rawMaximum + rawRange * 0.08;
  const priceRange = priceMaximum - priceMinimum;
  const maximumVolume = Math.max(...candles.map((candle) => candle.volume), 1);
  const plotWidth = WIDTH - PLOT_LEFT - PLOT_RIGHT;
  const candleStep = plotWidth / candles.length;
  const bodyWidth = Math.max(2, Math.min(11, candleStep * 0.62));
  const priceY = (price: number) =>
    PRICE_TOP + ((priceMaximum - price) / priceRange) * PRICE_HEIGHT;
  const latest = candles.at(-1)!;
  const timeTickIndexes = Array.from(
    new Set([0, Math.floor((candles.length - 1) / 2), candles.length - 1]),
  );

  return (
    <div className="chart-frame">
      <div className="chart-readout" aria-label="Latest candle values">
        <span>{formatTime(latest.timestamp)}</span>
        <span>
          O <strong>{formatPrice(latest.open)}</strong>
        </span>
        <span>
          H <strong>{formatPrice(latest.high)}</strong>
        </span>
        <span>
          L <strong>{formatPrice(latest.low)}</strong>
        </span>
        <span>
          C <strong>{formatPrice(latest.close)}</strong>
        </span>
        <span>
          Vol <strong>{formatVolume(latest.volume)}</strong>
        </span>
      </div>

      <svg
        className="candlestick-chart"
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="img"
        aria-label={`${candles.length} candle price and volume chart for ${latest.symbol}`}
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

        {candles.map((candle, index) => {
          const x = PLOT_LEFT + index * candleStep + candleStep / 2;
          const openY = priceY(candle.open);
          const closeY = priceY(candle.close);
          const rising = candle.close >= candle.open;
          const bodyTop = Math.min(openY, closeY);
          const bodyHeight = Math.max(1.8, Math.abs(closeY - openY));
          const volumeHeight = (candle.volume / maximumVolume) * VOLUME_HEIGHT;
          const direction = rising ? "up" : "down";

          return (
            <g className={`chart-candle chart-candle--${direction}`} key={candle.timestamp}>
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

        {timeTickIndexes.map((index) => {
          const x = PLOT_LEFT + index * candleStep + candleStep / 2;
          const anchor = index === 0 ? "start" : index === candles.length - 1 ? "end" : "middle";
          return (
            <text
              className="chart-axis-label"
              key={`time-${candles[index].timestamp}`}
              x={x}
              y={HEIGHT - 20}
              textAnchor={anchor}
            >
              {formatTime(candles[index].timestamp)}
            </text>
          );
        })}
      </svg>
    </div>
  );
}
