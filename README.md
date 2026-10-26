# argus

All-purpose intelligence platform — world surveillance (OSINT), markets/quant,
private-equity back-office — in Rust, dogfooding the [WyattAu](https://github.com/WyattAu)
crate estate. See the research brief: `../intel-platform-research-2026-10-24.md`.

**Status: M2 markets** — everything from M1 (six world-intel feeds + map) plus
**mdkit**: live Binance trade stream (WS, auto-reconnect) and 1m klines for
BTCUSDT/ETHUSDT/SOLUSDT, key-gated Alpaca equities quotes, `/api/v1/candles`,
candlestick panel with live-tick folding, and a Ctrl+K command palette
(`ch BTCUSDT`, `feed fire`, `help`).

```
feedkit (USGS + sim) ──▶ eventbus-kit bus ──▶ ws-kit BroadcastHub ──▶ ws://…/api/v1/stream
                              │                                        ▲
                              └──▶ state (ring buffer, quotes) ──▶ REST ┘
                                                          Astro+Solid island ◀┘
```

## Run

```sh
# core (binds 127.0.0.1:8420)
cargo run -p argus-server

# shell (binds localhost:4321, proxies /api incl. WS to the core)
cd web && bun install && bun run dev
```

Open http://localhost:4321 — markets chart (sim feed) + live USGS quake feed,
LIVE/STALE badge per panel. Config: `argus.toml` (+ `ARGUS_` env overrides).

## Endpoints

| Route | What |
|---|---|
| `GET /livez` `/ping` | liveness (`healthkit`) |
| `GET /readyz` | readiness w/ per-feed freshness checks |
| `GET /api/v1/observations?limit=` | recent world observations (`api-types` envelope) |
| `GET /api/v1/quotes` | latest quote per symbol |
| `GET /api/v1/candles?symbol=` | OHLCV history (`plycore::CandleData`), oldest first |
| `GET /api/v1/stream` | WebSocket: JSON `EventEnvelope<Event>` frames, CSWSH origin-checked |

Topics: `intel.<source>.<kind>`, `md.<source>.<symbol>` — wildcards `*`/`**`.

FIRMS is off by default: set `firms_map_key` in `argus.toml` (or
`ARGUS_FEEDS_FIRMS_MAP_KEY`) to a free NASA MAP_KEY, then `firms_enabled = true`.

## Dogfooded so far

config-kit · telemetry-init · healthkit · eventbus-kit · ws-kit · api-types ·
http-error · fetch-kit (+breaker) · breaker · plycore (CandleData in mdkit) ·
feedkit (7 adapters) · mdkit (new; Binance + Alpaca)

Web side: astro + @astrojs/solid-js + solid-js + maplibre-gl (demotiles).

## Dogfood notes (M0 findings)

1. **`eventbus` on crates.io is squatted** (SoniEx2) and holds the old sync bus; the
   async bus with persistence/replay is published as **`eventbus-kit`** — but its lib
   name is `typed_eventbus` (`use typed_eventbus::{EventBus, EventEnvelope}`).
   Consider unifying: publish the async bus under the `-kit` name with lib name
   `eventbus_kit`.
2. **`http-error` 0.2.0 has no derive macro** despite the repo description ("Derive
   macro for mapping error enums") — `HttpError` must be hand-implemented. Either ship
   the derive or fix the description.
3. **`fetch-kit` `with_breaker` is behind the `circuit-breaker` feature** — easy to
   miss; default features don't include it.
4. **`healthkit::axum::readiness_route` mounts itself at `/readyz`** — nest it at
   `/` (merge), not `/readyz`, or you 404. Methods return `Router`/`MethodRouter<()>`
   needing `.with_state(())` to merge into a stateful router.
5. **Astro dev proxy lives at `vite.server.proxy`** — neither Astro's `server.proxy`
   nor `vite.proxy` exists; both are silently ignored (`.mjs` configs aren't
   typechecked).
6. **`plychart` has no npm/wasm artifact yet** — the shell chart is a canvas stub.
   M1+ needs `wasm-pack` output (or an npm package) to swap in for real.
7. `eventbus-kit` `subscribe_sync` closures receive `Arc<EventEnvelope<T>>` while
   `BroadcastHub::broadcast` takes owned `T` — clone at the bridge (fine, documented).
8. `maplibre-gl`'s ESM build has **no default export** — import
   `{ Map, Marker, Popup, NavigationControl }` named, or rollup fails the build.
9. NVD keyless tier is ~5 req/30 s — keep `nvd_interval_secs >= 60` (default 120).
   FIRMS area API needs a MAP_KEY; adapter is `Option`-gated on it.
10. EONET categories map: wildfires→`Fire`, severeStorms→`Weather`,
    volcanoes→`Volcano`, everything else→`News`.
11. reqwest 0.13 renamed the `rustls-tls` feature to `rustls` — the old name
    fails version selection.
12. Binance combined streams use `<symbol>@<channel>`; `/` connects but
    delivers zero frames (silent). Always log the dialed URI.
13. tokio-tungstenite `split()`: Pongs queued on Ping are only flushed when
    you `send()` — handle `Message::Ping` explicitly or Binance drops you.

## Estate gates

- `cargo clippy --all-targets` green under the OmniRust-template tier-a posture
  (pedantic/unwrap/panic/indexing deny).
- `deny.toml` mirrors the estate policy; run `cargo deny check` before release.
- M1+: each milestone ends with `estate-integration` composition green.

## Roadmap

M3 ontology (ontokit, tantivy-ext search, graphkit, accessctl ACLs, auditlog) →
M2 markets (mdkit: Binance/Alpaca, plychart candles, command palette) →
M3 ontology (ontokit, tantivy-ext search, graphkit, accessctl ACLs, auditlog) →
M4 quant/alerts (alertkit → Telegram, plycompute) →
M5 PE/back-office (ledger-kit, recon-kit, invoice-kit panels) →
M6 surfaces (Tauri desktop, ratatui TUI, MCP server).
