// Entity inspector — search observations, inspect an entity's n-hop
// neighborhood as a radial SVG mini-graph. The M3 read surface for the
// ontology; link-analysis upgrades land with graphkit algorithms in M4.
import { createEffect, createSignal, For, Show } from "solid-js";

type SearchHit = { id: string; title: string; kind: string; source: string; score: number };
type Neighbor = { id: string; kind: string; name: string; depth: number; via: string; predicate: string };

const KIND_COLOR: Record<string, string> = {
  event: "#e5484d",
  place: "#00b4d8",
  source: "#90be6d",
  symbol: "#ffd166",
};

export default function EntityPanel() {
  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<SearchHit[]>([]);
  const [focused, setFocused] = createSignal<{ id: string; name: string } | null>(null);
  const [neighbors, setNeighbors] = createSignal<Neighbor[]>([]);

  const runSearch = async () => {
    const q = query().trim();
    if (q.length === 0) return;
    try {
      const res = await fetch(`/api/v1/search?q=${encodeURIComponent(q)}&limit=10`);
      const json = await res.json();
      setHits(json?.data ?? []);
    } catch {
      setHits([]);
    }
  };

  const focus = async (id: string, name: string) => {
    setFocused({ id, name });
    setNeighbors([]);
    try {
      const res = await fetch(
        `/api/v1/entities/${encodeURIComponent(id)}/neighbors?hops=2`,
      );
      const json = await res.json();
      setNeighbors(json?.data ?? []);
    } catch {
      setNeighbors([]);
    }
  };

  // Radial layout: focus at center, neighbors on an ellipse by index.
  const pos = (i: number, total: number) => {
    const angle = (2 * Math.PI * i) / Math.max(total, 1) - Math.PI / 2;
    return { x: 200 + 150 * Math.cos(angle), y: 150 + 110 * Math.sin(angle) };
  };

  return (
    <section class="panel span2">
      <div class="panel-head">
        <h2>ontology · entity inspector</h2>
        <span class="muted">search → focus → 2-hop neighborhood</span>
      </div>
      <div class="searchrow">
        <input
          class="searchbox"
          value={query()}
          placeholder='search observations — e.g. wildfire, alaska, cve'
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void runSearch();
          }}
        />
        <button onClick={() => void runSearch()}>search</button>
      </div>

      <Show when={hits().length > 0}>
        <ul class="hits">
          <For each={hits()}>
            {(h) => (
              <li>
                <button onClick={() => void focus(h.id, h.title)}>
                  [{h.kind}] {h.title.slice(0, 70)} <span class="muted">({h.source})</span>
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={focused()}>
        <div class="graphrow">
          <svg viewBox="0 0 400 300" class="minigraph">
            <For each={neighbors()}>
              {(n, i) => {
                const p = pos(i(), neighbors().length);
                const focusName = focused()?.name ?? "";
                return (
                  <>
                    <line x1="200" y1="150" x2={p.x} y2={p.y} stroke="#1e2530" stroke-width="1.5" />
                    <text
                      x={(200 + p.x) / 2}
                      y={(150 + p.y) / 2 - 3}
                      fill="#7c8698"
                      font-size="7"
                      text-anchor="middle"
                    >
                      {n.predicate}
                    </text>
                    <circle cx={p.x} cy={p.y} r="9" fill={KIND_COLOR[n.kind] ?? "#7c8698"} />
                    <text x={p.x} y={p.y + 22} fill="#d7dde8" font-size="8" text-anchor="middle">
                      {n.kind}·{n.name.slice(0, 18)}
                    </text>
                  </>
                );
              }}
            </For>
            <circle cx="200" cy="150" r="14" fill="#4cc38a" />
            <text x="200" y="178" fill="#4cc38a" font-size="9" text-anchor="middle">
              {focused()?.name.slice(0, 30)}
            </text>
          </svg>
          <ul class="neighbors">
            <For each={neighbors()}>
              {(n) => (
                <li>
                  <span class="chip" style={{ background: KIND_COLOR[n.kind] ?? "#7c8698" }}>
                    d{n.depth}
                  </span>
                  <span class="title">
                    {n.name.slice(0, 44)}
                    <span class="muted"> -{n.predicate}- via {n.via.slice(0, 26)}</span>
                  </span>
                  <button onClick={() => void focus(n.id, n.name)}>focus →</button>
                </li>
              )}
            </For>
            <Show when={neighbors().length === 0}>
              <li class="muted">no neighbors within 2 hops</li>
            </Show>
          </ul>
        </div>
      </Show>
    </section>
  );
}
