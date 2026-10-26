// Command palette evaluator — one grammar, every surface.
// Bloomberg-flavored: `ch <SYM>` / `sym <SYM>` chart a symbol, `feed <kind>`
// filters the intel feed. Pure function so the TUI/MCP surfaces can reuse
// the same contract later.

export type Command =
  | { type: "symbol"; symbol: string; message: string }
  | { type: "filter"; kind: string; message: string }
  | { type: "help"; message: string }
  | { type: "unknown"; message: string };

export const KNOWN_KINDS = ["quake", "fire", "weather", "volcano", "cve", "news"] as const;

const SYMBOL_RE = /^[A-Z0-9]{4,16}$/;

/** Parses one command line. Never throws — unknown input is a result. */
export function evaluate(input: string): Command {
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    return { type: "unknown", message: "empty command — try `help`" };
  }
  const parts = trimmed.split(/\s+/);
  const verb = parts[0]?.toLowerCase() ?? "";
  const arg = (parts[1] ?? "").toUpperCase();

  if (verb === "help" || verb === "?") {
    return {
      type: "help",
      message: "ch <SYM> | sym <SYM> — chart a symbol · feed <kind|all> — filter intel · help",
    };
  }
  if ((verb === "ch" || verb === "sym" || verb === "chart") && arg.length > 0) {
    if (!SYMBOL_RE.test(arg)) {
      return { type: "unknown", message: `not a symbol: ${arg}` };
    }
    return { type: "symbol", symbol: arg, message: `charting ${arg}` };
  }
  if (verb === "feed" && arg.length > 0) {
    const kind = arg.toLowerCase();
    if (kind !== "all" && !(KNOWN_KINDS as readonly string[]).includes(kind)) {
      return { type: "unknown", message: `unknown kind: ${kind} (try: ${KNOWN_KINDS.join(", ")}, all)` };
    }
    return { type: "filter", kind, message: `feed filter: ${kind}` };
  }
  return { type: "unknown", message: `unrecognized: ${trimmed} — try \`help\`` };
}
