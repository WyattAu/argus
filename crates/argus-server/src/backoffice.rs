//! Back-office domain: double-entry books, portfolio mark-to-market.
//!
//! The books run on the estate's `double-entry` (in-memory ledger, optimistic
//! versioning); reports come straight from `report-kit::TrialBalance`.
//! Portfolio positions mark against live quotes with the USDT→USD peg made
//! explicit through `fx-kit`'s exact-rational `Rate` restatement.
//!
//! Estate notes: `money` on crates.io is a squatted stub — the real
//! `Amount`/`Currency` live in `double_entry::money`.

use std::collections::BTreeMap;

use double_entry::money::{Amount, Currency};
use double_entry::{Account, AccountType, JournalEntry, Ledger, Line};
use fx_kit::Rate;
use serde::Serialize;
use vat_rules::TieMode;

/// Base currency for the books and portfolio.
pub const BASE_CURRENCY: &str = "USD";
/// USDT→USD peg; stablecoin, restated through fx-kit for exactness.
const USDT_USD_RATE: &str = "1.0";
/// Micro-units per whole unit (1e-6 precision on quantities).
const MICROS: i64 = 1_000_000;
/// Minor units per whole unit for 2-decimal currencies (float helper).
const MINOR_SCALE_F: f64 = 100.0;

/// One portfolio position from config.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Holding {
    /// Symbol as quoted on the venue (`BTCUSDT`).
    pub symbol: String,
    /// Quantity in micro-units (1e-6).
    pub qty_micros: i64,
    /// Cost basis in base-currency minor units.
    pub cost_minor: i64,
}

/// Seeds the books: chart of accounts plus opening entries.
#[must_use]
pub fn seed_ledger() -> Ledger {
    let mut ledger = Ledger::new();
    let accounts = [
        ("Assets:Cash:USD", "Cash (USD)", AccountType::Asset),
        ("Assets:Exchange:USD", "Exchange balance (USD)", AccountType::Asset),
        ("Equity:Capital:USD", "Capital introduced", AccountType::Equity),
        ("Income:Trading:USD", "Trading P&L", AccountType::Revenue),
        ("Expenses:Fees:USD", "Fees", AccountType::Expense),
    ];
    for (id, name, kind) in accounts {
        ledger.add_account(Account::new(id, name, kind));
    }

    let usd = currency(BASE_CURRENCY);
    let entries = [
        (
            "je-capital",
            "capital introduction",
            Line::debit("Assets:Cash:USD", Amount::minor(10_000_000, usd)),
            Line::credit("Equity:Capital:USD", Amount::minor(10_000_000, usd)),
        ),
        (
            "je-fund-exchange",
            "fund exchange account",
            Line::debit("Assets:Exchange:USD", Amount::minor(5_000_000, usd)),
            Line::credit("Assets:Cash:USD", Amount::minor(5_000_000, usd)),
        ),
        (
            "je-fee",
            "exchange fee",
            Line::debit("Expenses:Fees:USD", Amount::minor(999, usd)),
            Line::credit("Assets:Cash:USD", Amount::minor(999, usd)),
        ),
    ];
    for (id, memo, debit, credit) in entries {
        let entry = JournalEntry::draft(id, "2026-10-01", "2026-10", vec![debit, credit])
            .with_memo(memo);
        let version = ledger.version();
        if let Err(error) = ledger.post(&entry, version) {
            tracing::error!(%error, entry = id, "seed entry failed to post");
        }
    }
    ledger
}

fn currency(alpha: &str) -> Currency {
    // Static ISO table lookup; USD is the known-good fallback.
    double_entry::money::currencies::by_alpha(alpha).copied().unwrap_or(Currency {
        alpha: "USD",
        numeric: "840",
        minor_unit: Some(2),
    })
}

/// Venue price (f64) to base-currency minor units. Prices arrive as f64 by
/// nature of the feed; the ledger itself stays in exact minor units.
fn price_to_minor(price: f64) -> i64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    {
        (price * MINOR_SCALE_F).round() as i64
    }
}

/// Micro-unit quantity to a float for display rows.
#[allow(clippy::cast_precision_loss)]
fn qty_display(qty_micros: i64) -> f64 {
    qty_micros as f64 / MICROS as f64
}

fn iso_date(now_ms: i64) -> String {
    // Civil-from-days (Hinnant). Valid for the platform's lifetime.
    let days = now_ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn iso_month(now_ms: i64) -> String {
    let date = iso_date(now_ms);
    date.chars().take(7).collect()
}

/// Portfolio view rows and totals, base-currency minor units throughout.
#[derive(Debug, Clone, Serialize)]
pub struct PortfolioView {
    /// One row per holding.
    pub positions: Vec<PositionRow>,
    /// Total market value, minor units.
    pub value_minor: i64,
    /// Total cost basis, minor units.
    pub cost_minor: i64,
    /// Unrealized P&L, minor units.
    pub unrealized_minor: i64,
    /// Base currency code.
    pub base: String,
}

/// One marked position.
#[derive(Debug, Clone, Serialize)]
pub struct PositionRow {
    /// Symbol.
    pub symbol: String,
    /// Quantity (float rendering of micro-units).
    pub qty: f64,
    /// Last known price in base currency, minor units (None = no quote yet).
    pub last_minor: Option<i64>,
    /// Market value, minor units.
    pub value_minor: Option<i64>,
    /// Cost basis, minor units.
    pub cost_minor: i64,
    /// Unrealized P&L, minor units.
    pub unrealized_minor: Option<i64>,
}

/// Marks `holdings` against the latest quotes.
#[must_use]
pub fn portfolio(
    holdings: &[Holding],
    quotes: &BTreeMap<String, feedkit::Quote>,
    _now_ms: i64,
) -> PortfolioView {
    let rate = Rate::parse(USDT_USD_RATE).ok();
    let mut positions = Vec::new();
    let mut total_value = 0;
    let mut total_cost = 0;
    for holding in holdings {
        let quote = quotes.get(&holding.symbol.to_uppercase());
        let value_minor = quote.and_then(|quote| {
            let rate = rate.as_ref()?;
            let price_minor = price_to_minor(quote.price);
            // qty in micro-units x price in minor units, rescaled to minor
            // units, then restated USDT->USD through the exact peg rate.
            let usdt_minor = holding.qty_micros.saturating_mul(price_minor) / MICROS;
            Some(rate.restate(usdt_minor, TieMode::HalfUp))
        });
        let unrealized = value_minor.map(|value| value - holding.cost_minor);
        if let Some(value) = value_minor {
            total_value += value;
        }
        total_cost += holding.cost_minor;
        positions.push(PositionRow {
            symbol: holding.symbol.clone(),
            qty: qty_display(holding.qty_micros),
            last_minor: quote.map(|q| price_to_minor(q.price)),
            value_minor,
            cost_minor: holding.cost_minor,
            unrealized_minor: unrealized,
        });
    }
    PortfolioView {
        positions,
        value_minor: total_value,
        cost_minor: total_cost,
        unrealized_minor: total_value - total_cost,
        base: BASE_CURRENCY.to_string(),
    }
}

/// Manual posting request.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManualPost {
    /// Account to debit.
    pub debit_account: String,
    /// Account to credit.
    pub credit_account: String,
    /// Amount in base-currency minor units.
    pub amount_minor: i64,
    /// Memo for the journal entry.
    pub memo: String,
}

/// Posts a balanced two-line entry; returns the new ledger version.
///
/// # Errors
/// Returns the `PostError` message when the entry is unbalanced, accounts
/// are missing, or the version check fails.
pub fn post_manual(ledger: &mut Ledger, request: &ManualPost, now_ms: i64) -> Result<u64, String> {
    let usd = currency(BASE_CURRENCY);
    let seq = ledger.version();
    let entry = JournalEntry::draft(
        format!("je-manual-{now_ms}"),
        iso_date(now_ms),
        iso_month(now_ms),
        vec![
            Line::debit(request.debit_account.clone(), Amount::minor(request.amount_minor, usd))
                .with_memo(request.memo.clone()),
            Line::credit(request.credit_account.clone(), Amount::minor(request.amount_minor, usd))
                .with_memo(request.memo.clone()),
        ],
    )
    .with_memo(request.memo.clone());
    ledger.post(&entry, seq).map_err(|error| error.to_string())
}


