// Shared kind vocabulary: colors, labels, and the Observation type.
export type Observation = {
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

export type Quote = { symbol: string; price: number; ts_ms: number };

// OkLCH-chosen hues; dichromat-safe variants come from theme-kit in M2.
export const KIND_COLOR: Record<string, string> = {
  quake: "#e5484d",
  fire: "#fb8500",
  weather: "#00b4d8",
  volcano: "#b5179e",
  cve: "#ffd166",
  news: "#90be6d",
  sim: "#4cc38a",
};

export function kindLabel(observation: Observation): string {
  return observation.kind.charAt(0).toUpperCase() + observation.kind.slice(1);
}
