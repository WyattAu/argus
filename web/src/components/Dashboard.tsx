// M0 dashboard island: markets chart + world-intel feed + LIVE/STALE badge.
// Panel data flows over ONE websocket mirroring the server event bus.
// plychart replaces the canvas stub once it ships a wasm/npm artifact.
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";

type Quote = { symbol: string; price: number; ts_ms: number };
type Observation = {
  id: string;
  source: string;
  kind: string;
  title: string;
  place: string | null;
  lat: number | null;
  lon: number | null;
  magnitude: number | null;
  url: string | null;
  occurred_at_ms: number;
  fetched_at_ms: number;
  license: string;
};
type Envelope = { id: string; topic: string; timestamp: number; payload: { type: string } };

const HISTORY_POINTS = 120;
const STALE_AFTER_MS = 6000;

export default function Dashboard() {
  const [stale, setStale] = createSignal(true);
  const [lastMsgAt, setLastMsgAt] = createSignal(0);
  const [quotes, setQuotes] = createSignal<Record<string, Quote>>({});
  const [observations, setObservations] = createSignal<Observation[]>([]);
  // symbol -> rolling price history (AAPL is the charted series in M0).
  const history = new Map<string, number[]>();
  const [chartTick, setChartTick] = createSignal(0);
  let canvas: HTMLCanvasElement | undefined;

  const noteMessage = () => {
    setLastMsgAt(Date.now());
    setStale(false);
  };

  onMount(async () => {
    const seed = await fetch("/api/v1/observations?limit=25").then((r) => r.json());
    if (seed?.data) setObservations(seed.data);

    const proto = location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${proto}://${location.host}/api/v1/stream`);
    ws.onmessage = (e) => {
      noteMessage();
      let envelope: Envelope;
      try {
        envelope = JSON.parse(e.data);
      } catch {
        return;
      }
      const payload = envelope.payload as unknown;
      if (envelope.topic.startsWith("md.") && isQuote(payload)) {
        setQuotes((prev) => ({ ...prev, [payload.symbol]: payload }));
        pushPrice(payload.symbol, payload.price);
        setChartTick((t) => t + 1);
      } else if (envelope.topic.startsWith("intel.") && isObservation(payload)) {
        setObservations((prev) => {
          if (prev.some((o) => o.id === payload.id)) return prev;
          return [payload, ...prev].slice(0, 25);
        });
      }
    };
    ws.onclose = () => setStale(true);
    onCleanup(() => ws.close());
  });

  const isQuote = (p: unknown): p is Quote =>
    !!p && typeof p === "object" && "symbol" in p && "price" in p;
  const isObservation = (p: unknown): p is Observation =>
    !!p && typeof p === "object" && "id" in p && "title" in p && "kind" in p;

  const pushPrice = (symbol: string, price: number) => {
    const series = history.get(symbol) ?? [];
    series.push(price);
    if (series.length > HISTORY_POINTS) series.shift();
    history.set(symbol, series);
  };

  // Redraw the chart whenever new quotes land.
  createEffect(() => {
    chartTick();
    drawChart(canvas, history.get("AAPL") ?? []);
  });

  // Freshness watchdog: bus quiet for too long -> STALE.
  const watchdog = setInterval(() => {
    if (Date.now() - lastMsgAt() > STALE_AFTER_MS) setStale(true);
  }, 2000);
  onCleanup(() => clearInterval(watchdog));

  return (
    <div class="grid">
      <section class="panel span2">
        <div class="panel-head">
          <h2>markets · md.sim.aapl</h2>
          <span class={`badge ${stale() ? "stale" : "live"}`}>
            {stale() ? "● DATA STALE" : "● LIVE"}
          </span>
        </div>
        <canvas ref={canvas} width="640" height="180" />
        <div class="quotes">
          <For each={Object.values(quotes())}>
            {(q) => (
              <span class="quote">
                {q.symbol} {q.price.toFixed(q.price < 10 ? 4 : 2)}
              </span>
            )}
          </For>
        </div>
      </section>

      <section class="panel span2">
        <div class="panel-head">
          <h2>world intel · intel.usgs.quake</h2>
          <span class="muted">usgs all_hour · attribution in payload</span>
        </div>
        <ul class="feed">
          <For each={observations()}>
            {(o) => (
              <li>
                <span class="mag">{o.magnitude != null ? `M ${o.magnitude.toFixed(1)}` : "—"}</span>
                <span class="title">{o.title}</span>
                <Show when={o.url}>
                  <a href={o.url ?? "#"} target="_blank" rel="noreferrer">
                    source ↗
                  </a>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </section>
    </div>
  );
}

function drawChart(canvas: HTMLCanvasElement | undefined, series: number[]) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const { width, height } = canvas;
  ctx.clearRect(0, 0, width, height);
  if (series.length < 2) return;
  const min = Math.min(...series);
  const max = Math.max(...series);
  const span = max - min || 1;
  const step = width / (HISTORY_POINTS - 1);
  const offset = HISTORY_POINTS - series.length;
  ctx.strokeStyle = "#4cc38a";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  series.forEach((price, i) => {
    const x = (offset + i) * step;
    const y = height - 12 - ((price - min) / span) * (height - 24);
    if (i === 0 && offset === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
  ctx.fillStyle = "#7c8698";
  ctx.font = "11px ui-monospace, monospace";
  ctx.fillText(min.toFixed(2), 4, height - 2);
  ctx.fillText(max.toFixed(2), 4, 10);
}
