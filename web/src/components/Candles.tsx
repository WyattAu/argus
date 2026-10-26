// Candlestick panel — OHLCV history for one symbol, with a live last-price
// update from the trade stream. Canvas stub until plychart ships its
// wasm/npm artifact (same swap path as the line chart).
import { createEffect, createSignal, For, onCleanup, onMount } from "solid-js";

type Candle = { time: number; open: number; high: number; low: number; close: number; volume: number };

export default function Candles(props: {
  symbol: () => string;
  onSymbol: (symbol: string) => void;
  lastPrice: () => Record<string, number>;
}) {
  const [candles, setCandles] = createSignal<Candle[]>([]);
  let canvas: HTMLCanvasElement | undefined;

  const load = async (symbol: string) => {
    try {
      const res = await fetch(`/api/v1/candles?symbol=${encodeURIComponent(symbol)}&limit=200`);
      const json = await res.json();
      if (json?.data) setCandles(json.data);
    } catch {
      /* transient — the next refresh cycle retries */
    }
  };

  onMount(() => {
    void load(props.symbol());
    const poll = setInterval(() => void load(props.symbol()), 15000);
    onCleanup(() => clearInterval(poll));
  });

  createEffect(() => {
    props.symbol();
    void load(props.symbol());
  });

  // Live tick: fold the newest trade into the last candle and redraw.
  createEffect(() => {
    const price = props.lastPrice()[props.symbol()];
    if (price == null) return;
    setCandles((prev) => {
      if (prev.length === 0) return prev;
      const last = prev[prev.length - 1];
      const patched: Candle = {
        ...last,
        close: price,
        high: Math.max(last.high, price),
        low: Math.min(last.low, price),
      };
      return [...prev.slice(0, -1), patched];
    });
  });

  createEffect(() => {
    drawCandles(canvas, candles());
  });

  const symbols = ["BTCUSDT", "ETHUSDT", "SOLUSDT"];

  return (
    <section class="panel">
      <div class="panel-head">
        <h2>candles · binance 1m</h2>
        <div class="switcher">
          <For each={symbols}>
            {(s) => (
              <button
                class={`sym ${props.symbol() === s ? "active" : ""}`}
                onClick={() => props.onSymbol(s)}
              >
                {s}
              </button>
            )}
          </For>
        </div>
      </div>
      <canvas ref={canvas} width="640" height="200" />
      <div class="muted">
        {props.symbol()} · {candles().length} candles · live close via trade stream
      </div>
    </section>
  );
}

function drawCandles(canvas: HTMLCanvasElement | undefined, candles: Candle[]) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx || candles.length < 2) return;
  const { width, height } = canvas;
  ctx.clearRect(0, 0, width, height);
  const highs = candles.map((c) => c.high);
  const lows = candles.map((c) => c.low);
  const max = Math.max(...highs);
  const min = Math.min(...lows);
  const span = max - min || 1;
  const slot = width / candles.length;
  const bodyW = Math.max(2, slot * 0.6);
  const y = (price: number) => 8 + (1 - (price - min) / span) * (height - 24);
  for (let i = 0; i < candles.length; i++) {
    const c = candles[i];
    const up = c.close >= c.open;
    ctx.strokeStyle = up ? "#4cc38a" : "#e5484d";
    ctx.fillStyle = up ? "#4cc38a" : "#e5484d";
    const cx = i * slot + slot / 2;
    // wick
    ctx.beginPath();
    ctx.moveTo(cx, y(c.high));
    ctx.lineTo(cx, y(c.low));
    ctx.stroke();
    // body
    const top = y(Math.max(c.open, c.close));
    const bottom = y(Math.min(c.open, c.close));
    ctx.fillRect(cx - bodyW / 2, top, bodyW, Math.max(1, bottom - top));
  }
  ctx.fillStyle = "#7c8698";
  ctx.font = "11px ui-monospace, monospace";
  ctx.fillText(max.toFixed(2), 4, 12);
  ctx.fillText(min.toFixed(2), 4, height - 2);
}
