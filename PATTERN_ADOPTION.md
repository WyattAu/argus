# PATTERN_ADOPTION: argus

Relationship to the WyattAu pattern canon (`references/catalog.md` in
pattern-standards). Rows are code-verified; checked by
`pattern-standards/scripts/adoption-check.py`.

| Pattern | Status | Evidence |
|---|---|---|
| T20 | partial | crates/argus-server/src/state.rs:44 (bounded ring buffer for recent observations; RwLock-guarded rather than single-writer) |
| T21 | adopted | crates/argus-server/src/backoffice.rs:316 (reload rebuilds the ledger by re-posting the append-only journal entries in file order), crates/argus-server/src/backoffice.rs:367 (snapshot persisted with atomic tmp plus rename) |
| T27 | adopted | crates/argus-server/src/backoffice.rs:59 (balanced debit and credit postings on the double_entry ledger; trial balance comes from report-kit) |
| P21 | adopted | crates/argus-server/src/main.rs:236 (per-feed staleness budgets from config gate each feed's readyz health to Degraded) |
| P55 | adopted | crates/argus-server/src/backoffice.rs:242 (manual postings submit the ledger version read at crates/argus-server/src/backoffice.rs:231; a mismatch fails the post) |
| P62 | adopted | crates/argus-server/src/main.rs:132 (axum graceful shutdown on ctrl-C and SIGTERM) |
| P72 | adopted | crates/argus-server/src/api.rs:57 (api_types ApiResponse envelope on success), crates/argus-server/src/error.rs:12 (closed ServerError set with stable machine codes) |
| P83 | adopted | crates/argus-server/src/api.rs:299 (provenance head, verify, and tail over the ontokit EntityStore mutation chain, held at crates/argus-server/src/state.rs:27) |
| P114 | partial | crates/argus-server/src/authz.rs:102 (JWT mode: mandatory bearer token, typed claims decode, role gate; trusted dev headers only when no JWT secret is configured, crates/argus-server/src/authz.rs:114) |
| P42 | refused | R6 (single-node server with per-handler tracing spans; no wide-event accumulator until request-correlation pain appears) |
| P45 | planned | R6 (no SLO targets defined; alerting is alertkit rules to Telegram, crates/argus-server/src/feeds.rs:102) |
| P63 | refused | R6 (single in-process event bus; requests do not cross service hops to correlate) |
