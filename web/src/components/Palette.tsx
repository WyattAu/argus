// Command palette overlay — Ctrl+K or `/`. Evaluates one line through
// palette.ts and dispatches the resulting action.
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { evaluate, type Command } from "./palette";

export default function Palette(props: {
  onSymbol: (symbol: string) => void;
  onFilter: (kind: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [line, setLine] = createSignal("");
  const [status, setStatus] = createSignal("");
  let input: HTMLInputElement | undefined;

  const openPalette = () => {
    setOpen(true);
    setStatus("");
    queueMicrotask(() => input?.focus());
  };

  const dispatch = (command: Command) => {
    setStatus(command.message);
    if (command.type === "symbol") props.onSymbol(command.symbol);
    if (command.type === "filter") props.onFilter(command.kind);
  };

  const submit = () => {
    dispatch(evaluate(line()));
    setLine("");
  };

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        openPalette();
      } else if (e.key === "/" && !open() && !(e.target instanceof HTMLInputElement)) {
        e.preventDefault();
        openPalette();
      } else if (e.key === "Escape" && open()) {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  return (
    <Show when={open()}>
      <div class="palette-backdrop" onClick={() => setOpen(false)}>
        <div class="palette" onClick={(e) => e.stopPropagation()}>
          <input
            ref={input}
            value={line()}
            placeholder="ch BTCUSDT · feed fire · help"
            onInput={(e) => setLine(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
          />
          <Show when={status()}>
            <div class="palette-status">{status()}</div>
          </Show>
        </div>
      </div>
    </Show>
  );
}
