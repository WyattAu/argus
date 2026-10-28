// Back-office panel: portfolio mark-to-market + trial balance + manual
// posting (editor role). Minor units render as decimal strings server-side
// conventions: all *_minor fields are base-currency 1e-2 units.
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";

type Position = {
  symbol: string;
  qty: number;
  last_minor: number | null;
  value_minor: number | null;
  cost_minor: number;
  unrealized_minor: number | null;
};
type Portfolio = {
  positions: Position[];
  value_minor: number;
  cost_minor: number;
  unrealized_minor: number;
  base: string;
};
type TrialRow = { account: string; name: string; debit_minor: number; credit_minor: number; net_minor: number };
type TrialBalance = { balanced: boolean; accounting_equation_holds: boolean; rows: TrialRow[] };

const fmt = (minor: number | null | undefined): string =>
  minor == null ? "—" : (minor / 100).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export default function FinanceOps() {
  const [portfolio, setPortfolio] = createSignal<Portfolio | null>(null);
  const [trial, setTrial] = createSignal<TrialBalance | null>(null);
  const [status, setStatus] = createSignal("");
  const [debit, setDebit] = createSignal("Assets:Cash:USD");
  const [credit, setCredit] = createSignal("Income:Trading:USD");
  const [amount, setAmount] = createSignal("125.00");
  const [memo, setMemo] = createSignal("");

  const role = () => localStorage.getItem("argus-role") ?? "viewer";

  const loadPortfolio = async () => {
    try {
      const res = await fetch("/api/v1/backoffice/portfolio");
      const json = await res.json();
      if (json?.data) setPortfolio(json.data);
    } catch {
      /* transient */
    }
  };
  const loadTrial = async () => {
    try {
      const res = await fetch("/api/v1/backoffice/trial-balance");
      const json = await res.json();
      if (json?.data) setTrial(json.data);
    } catch {
      /* transient */
    }
  };

  onMount(() => {
    void loadPortfolio();
    void loadTrial();
    const poll = setInterval(() => void loadPortfolio(), 10000);
    onCleanup(() => clearInterval(poll));
  });

  const post = async () => {
    const amountMinor = Math.round(Number.parseFloat(amount()) * 100);
    if (!Number.isFinite(amountMinor) || amountMinor <= 0) {
      setStatus("amount must be positive");
      return;
    }
    try {
      const res = await fetch("/api/v1/backoffice/post", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-argus-role": role() === "viewer" ? "editor" : role(),
        },
        body: JSON.stringify({
          debit_account: debit(),
          credit_account: credit(),
          amount_minor: amountMinor,
          memo: memo() || "manual posting",
        }),
      });
      const json = await res.json();
      setStatus(
        res.ok ? `posted (ledger v${json?.data?.ledger_version})` : (json?.error?.message ?? `HTTP ${res.status}`),
      );
      if (res.ok) void loadTrial();
    } catch (error) {
      setStatus(`post failed: ${String(error)}`);
    }
  };

  return (
    <section class="panel span2">
      <div class="panel-head">
        <h2>back office · {portfolio()?.base ?? "USD"}</h2>
        <span class="muted">double-entry books · report-kit trial balance</span>
      </div>
      <div class="opsgrid">
        <div>
          <h3>portfolio (marked to market)</h3>
          <table>
            <thead>
              <tr><th>symbol</th><th>qty</th><th>last</th><th>value</th><th>cost</th><th>p&l</th></tr>
            </thead>
            <tbody>
              <For each={portfolio()?.positions ?? []}>
                {(p) => (
                  <tr>
                    <td>{p.symbol}</td>
                    <td>{p.qty}</td>
                    <td>{fmt(p.last_minor)}</td>
                    <td>{fmt(p.value_minor)}</td>
                    <td>{fmt(p.cost_minor)}</td>
                    <td class={(p.unrealized_minor ?? 0) >= 0 ? "pos" : "neg"}>{fmt(p.unrealized_minor)}</td>
                  </tr>
                )}
              </For>
            </tbody>
            <tfoot>
              <tr>
                <td colspan="4">total</td>
                <td>{fmt(portfolio()?.cost_minor)}</td>
                <td class={(portfolio()?.unrealized_minor ?? 0) >= 0 ? "pos" : "neg"}>
                  {fmt(portfolio()?.unrealized_minor)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>

        <div>
          <h3>
            trial balance{" "}
            <Show when={trial()}>
              <span class={trial()?.balanced ? "pos" : "neg"}>
                {trial()?.balanced ? "✓ balanced" : "✗ imbalanced"}
                {" · "}
                {trial()?.accounting_equation_holds ? "equation holds" : "equation broken"}
              </span>
            </Show>
          </h3>
          <table>
            <thead>
              <tr><th>account</th><th>debit</th><th>credit</th><th>net</th></tr>
            </thead>
            <tbody>
              <For each={trial()?.rows ?? []}>
                {(r) => (
                  <tr>
                    <td>{r.account}</td>
                    <td>{fmt(r.debit_minor)}</td>
                    <td>{fmt(r.credit_minor)}</td>
                    <td>{fmt(r.net_minor)}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>

          <div class="postform">
            <input value={debit()} onInput={(e) => setDebit(e.currentTarget.value)} placeholder="debit account" />
            <input value={credit()} onInput={(e) => setCredit(e.currentTarget.value)} placeholder="credit account" />
            <input value={amount()} onInput={(e) => setAmount(e.currentTarget.value)} placeholder="125.00" />
            <input value={memo()} onInput={(e) => setMemo(e.currentTarget.value)} placeholder="memo" />
            <button onClick={() => void post()}>post</button>
            <Show when={status()}>
              <span class="muted">{status()}</span>
            </Show>
          </div>
        </div>
      </div>
    </section>
  );
}
