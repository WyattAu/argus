// MapLibre panel — geo observations on a world map (kind-colored markers).
// Style: MapLibre demotiles (free, attribution required). Swap for a proper
// vector style + leptos-leaflet parity decisions in M2.
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { Map as MlMap, Marker, NavigationControl, Popup } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";
import { KIND_COLOR, kindLabel, type Observation } from "./kinds";

const MAX_MARKERS = 150;

export default function MapPanel(props: { observations: () => Observation[] }) {
  let container: HTMLDivElement | undefined;
  const [ready, setReady] = createSignal(false);
  let map: MlMap | undefined;
  const markers = new Map<string, Marker>();

  onMount(() => {
    if (!container) return;
    map = new MlMap({
      container,
      style: "https://demotiles.maplibre.org/style.json",
      center: [10, 20],
      zoom: 1.2,
      attributionControl: true,
    });
    map.addControl(new NavigationControl({ showCompass: false }), "top-right");
    map.on("load", () => setReady(true));
    onCleanup(() => map?.remove());
  });

  createEffect(() => {
    const observations = props.observations();
    if (!ready() || !map) return;
    const geo = observations.filter((o) => o.lat != null && o.lon != null).slice(0, MAX_MARKERS);
    const live = new Set<string>();
    for (const o of geo) {
      live.add(o.id);
      if (markers.has(o.id)) continue;
      const el = document.createElement("div");
      el.className = "argus-marker";
      el.style.background = KIND_COLOR[o.kind] ?? "#7c8698";
      el.title = o.title;
      el.addEventListener("click", () => {
        new Popup({ offset: 12, maxWidth: "320px" })
          .setLngLat([o.lon ?? 0, o.lat ?? 0])
          .setHTML(
            `<strong>${escapeHtml(o.title)}</strong><br/>` +
              `<span style="color:#7c8698">${kindLabel(o)} · ${o.source} · ${o.license}</span>` +
              (o.url ? `<br/><a href="${o.url}" target="_blank" rel="noreferrer">source ↗</a>` : ""),
          )
          .addTo(map as MlMap);
      });
      markers.set(
        o.id,
        new Marker({ element: el }).setLngLat([o.lon ?? 0, o.lat ?? 0]).addTo(map as MlMap),
      );
    }
    // Drop markers whose observations scrolled out of the ring buffer.
    for (const [id, marker] of markers) {
      if (!live.has(id)) {
        marker.remove();
        markers.delete(id);
      }
    }
  });

  return (
    <div class="panel span2">
      <div class="panel-head">
        <h2>world map · geo observations</h2>
        <span class="muted">demotiles · {MAX_MARKERS} marker cap</span>
      </div>
      <div ref={container} class="map" />
    </div>
  );
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
