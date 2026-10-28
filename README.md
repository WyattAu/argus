# argus

All-purpose intelligence platform — world surveillance (OSINT), markets/quant,
private-equity back-office — in Rust, dogfooding the [WyattAu](https://github.com/WyattAu)
crate estate. See the research brief: `../intel-platform-research-2026-10-24.md`.

**Status: M5 back-office** — everything from M1–M4 plus the finance row:
**double-entry books** seeded with a chart of accounts (report-kit trial
balance, accounting-equation check), a **portfolio marked to market** against
live quotes (USDT→USD peg restated through fx-kit's exact `Rate`), and an
editor-gated manual posting endpoint. Panels: portfolio table, trial balance,
manual posting form.

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
| `GET /api/v1/search?q=` | full-text over observations (tantivy) |
| `GET /api/v1/entities?kind=` | ontology entities, newest-seen first |
| `GET /api/v1/entities/{id}/neighbors?hops=` | n-hop neighborhood, entity-joined |
| `GET /api/v1/provenance` | chain head + verify + recent mutations |
| `GET /api/v1/backoffice/portfolio` | holdings marked to live quotes (fx-kit peg) |
| `GET /api/v1/backoffice/trial-balance` | report-kit trial balance + equation check |
| `POST /api/v1/backoffice/post` | manual balanced posting — **editor+ only** |

Back-office reads need `viewer`; `POST /api/v1/backoffice/post` needs
`editor` (send `x-argus-role: editor` in dev mode, or mint an editor token).
Ontology endpoints are gated. Two modes:

- **Dev (default)** — `auth.jwt_secret` empty: `x-argus-role` headers trusted.
- **JWT** — set `auth.jwt_secret` (or `ARGUS_AUTH_JWT_SECRET`): mint via
  `POST /api/v1/auth/token` with header `x-argus-mint-key: <secret>` and body
  `{user, role, ttl_secs}`; call APIs with `Authorization: Bearer <token>`.
  Missing/invalid tokens get 401, wrong roles 403.
| `GET /api/v1/stream` | WebSocket: JSON `EventEnvelope<Event>` frames, CSWSH origin-checked |

Topics: `intel.<source>.<kind>`, `md.<source>.<symbol>` — wildcards `*`/`**`.

FIRMS is off by default: set `firms_map_key` in `argus.toml` (or
`ARGUS_FEEDS_FIRMS_MAP_KEY`) to a free NASA MAP_KEY, then `firms_enabled = true`.

## Dogfooded so far

config-kit · telemetry-init · healthkit · eventbus-kit · ws-kit · api-types ·
http-error · fetch-kit (+breaker) · breaker · plycore (CandleData in mdkit) ·
accessctl (Role::has_at_least gate) · tokenkit (JwtService mint/verify) ·
double-entry + report-kit (books, trial balance) · fx-kit + vat-rules
(TieMode peg restatement) · feedkit (8 event types incl. Alert) ·
feedkit adapters (7) · mdkit (Binance + Alpaca) · graphkit + ontokit ·
alertkit (rules, cooldowns, Telegram)

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
14. accessctl 0.1.3: `cedar::authorize` exists but the `cedar` module is
    **private** (README shows the call anyway). Published path =
    `Role::has_at_least` / `RoleHierarchy`. The JWT `RequireRole`
    extractor (tokenkit feature) is the M4 authN hook.
15. `tantivy-ext` on crates.io is graysonrie's, not ours (collision) — M3
    uses raw tantivy 0.27 behind `argus-server/src/search.rs`; rename
    tantivy-ext before it can back this.
16. `auditlog` likewise collides (JijoBose) — ontokit ships a minimal
    SHA-256 chain (`ProvenanceChain`) as the stand-in.
17. tantivy 0.27: `TopDocs` is no longer a `Collector` — call
    `.order_by_score()`; `IndexWriter::commit` takes `&mut self`; document
    values come back as `CompactDocValue` (convert via `OwnedValue::from`).
18. Regression lesson: `#[derive(Default)]` on a type whose `new()`
    establishes an invariant (genesis hash) silently skips the invariant
    — one store consumed `default()`. Test the default path too.
19. tokenkit 1.0: `JwtConfig` has a full `Default` (HS256, exp+iss
    required) — struct-update syntax; set `issuer: Some(...)` or decode
    rejects your own tokens. `decode(&str)`, `encode(&claims)` generic.
20. Binance combined-stream silence is self-inflicted 90% of the time:
    log the dialed URI before blaming the network.
21. `money` on crates.io is a squatted stub — the real `Amount`/`Currency`
    live in `double_entry::money` (and the ISO table is at
    `money::currencies::by_alpha`, not `Currency::by_alpha`).
22. fx-kit's `TieMode` re-exports nothing: you need `vat-rules` as a
    direct dependency to name `TieMode::HalfUp`.
23. report-kit reads `double_entry::Ledger` directly — keep your
    double-entry version compatible with report-kit's pin.

## Estate gates

- `cargo clippy --all-targets` green under the OmniRust-template tier-a posture
  (pedantic/unwrap/panic/indexing deny).
- `deny.toml` mirrors the estate policy; run `cargo deny check` before release.
- M1+: each milestone ends with `estate-integration` composition green.

## Roadmap

M6 surfaces (Tauri desktop, ratatui TUI, MCP server) — then rename-and-publish
pass for the collided crates (money, tantivy-ext, auditlog, eventbus).
M2 markets (mdkit: Binance/Alpaca, plychart candles, command palette) →
M3 ontology (ontokit, tantivy-ext search, graphkit, accessctl ACLs, auditlog) →
M4 quant/alerts (alertkit → Telegram, plycompute) →
M5 PE/back-office (ledger-kit, recon-kit, invoice-kit panels) →
M6 surfaces (Tauri desktop, ratatui TUI, MCP server).
