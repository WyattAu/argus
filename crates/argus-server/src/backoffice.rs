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
            Line::debit(request.debit_account.clone(), Amount::minor(request.amount_minor, usd)),
            Line::credit(request.credit_account.clone(), Amount::minor(request.amount_minor, usd)),
        ],
    )
    .with_memo(request.memo.clone());
    ledger.post(&entry, seq).map_err(|error| error.to_string())
}

/// The chart of accounts every snapshot reloads before re-posting.
const CHART_OF_ACCOUNTS: [(&str, &str, AccountType); 5] = [
    ("Assets:Cash:USD", "Cash (USD)", AccountType::Asset),
    (
        "Assets:Exchange:USD",
        "Exchange balance (USD)",
        AccountType::Asset,
    ),
    ("Equity:Capital:USD", "Capital introduced", AccountType::Equity),
    ("Income:Trading:USD", "Trading P&L", AccountType::Revenue),
    ("Expenses:Fees:USD", "Fees", AccountType::Expense),
];

/// Serialized journal entry for the snapshot file.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SnapshotEntry {
    id: String,
    date: String,
    period: String,
    memo: Option<String>,
    lines: Vec<SnapshotLine>,
}

/// One serialized line.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SnapshotLine {
    account: String,
    side: String,
    units: i64,
    scale: u32,
    currency: String,
}

/// Serializes all posted entries to a JSON snapshot string.
///
/// # Errors
/// Returns the serde error on serialization failure.
pub fn snapshot(ledger: &Ledger) -> Result<String, serde_json::Error> {
    let entries: Vec<SnapshotEntry> = ledger
        .posted_entries()
        .iter()
        .map(|entry| SnapshotEntry {
            id: entry.id.clone(),
            date: entry.date.clone(),
            period: entry.period.clone(),
            memo: entry.memo.clone(),
            lines: entry
                .lines
                .iter()
                .map(|line| SnapshotLine {
                    account: line.account.as_str().to_string(),
                    side: match line.side {
                        double_entry::Side::Debit => "debit".to_string(),
                        double_entry::Side::Credit => "credit".to_string(),
                    },
                    units: line.amount.units,
                    scale: line.amount.scale,
                    currency: line.amount.currency.alpha.to_string(),
                })
                .collect(),
        })
        .collect();
    serde_json::to_string_pretty(&entries)
}

/// Rebuilds a ledger from a snapshot: chart of accounts, then re-posted
/// entries in file order (the snapshot is append-only by construction).
///
/// # Errors
/// Returns a message when the snapshot is malformed or an entry fails to
/// re-post (imbalance, missing account).
pub fn reload(snapshot: &str) -> Result<Ledger, String> {
    let entries: Vec<SnapshotEntry> =
        serde_json::from_str(snapshot).map_err(|error| error.to_string())?;
    let mut ledger = Ledger::new();
    for (id, name, kind) in CHART_OF_ACCOUNTS {
        ledger.add_account(Account::new(id, name, kind));
    }
    for entry in &entries {
        let lines: Vec<Line> = entry
            .lines
            .iter()
            .map(|line| {
                let amount = Amount {
                    units: line.units,
                    scale: line.scale,
                    rounded_to: None,
                    currency: currency(&line.currency),
                };
                match line.side.as_str() {
                    "debit" => Line::debit(line.account.clone(), amount),
                    _ => Line::credit(line.account.clone(), amount),
                }
            })
            .collect();
        let mut posted =
            JournalEntry::draft(entry.id.clone(), entry.date.clone(), entry.period.clone(), lines);
        if let Some(memo) = &entry.memo {
            posted = posted.with_memo(memo.clone());
        }
        let version = ledger.version();
        ledger
            .post(&posted, version)
            .map_err(|error| error.to_string())?;
    }
    Ok(ledger)
}

/// Writes the snapshot to disk (atomic tmp+rename replace).
///
/// # Errors
/// Returns I/O or serde errors.
pub fn save_snapshot(
    ledger: &Ledger,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = snapshot(ledger)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Serializable reconciliation report.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconView {
    /// Statement currency.
    pub currency: String,
    /// Declared opening balance, minor units.
    pub opening_balance: i64,
    /// Declared closing balance, minor units.
    pub closing_balance: i64,
    /// Opening plus sum of signed lines.
    pub computed_closing: i64,
    /// Whether the balance identity held.
    pub valid: bool,
    /// Amount+date+direction exact matches.
    pub exact: Vec<ReconMatch>,
    /// Dated-window proposals (single or ambiguous).
    pub proposals: Vec<ReconProposal>,
    /// Lines nothing in the books explains.
    pub unmatched: Vec<ReconLine>,
}

/// One confirmed exact match.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconMatch {
    /// Statement line id (`FITID`).
    pub line_id: String,
    /// Journal entry id.
    pub entry_id: String,
}

/// One window proposal.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReconProposal {
    /// Single plausible entry.
    Match {
        /// Statement line id.
        line_id: String,
        /// Journal entry id.
        entry_id: String,
    },
    /// Several entries are equally plausible; surfaced, not resolved.
    Ambiguous {
        /// Statement line id.
        line_id: String,
        /// Candidate entry ids, closest date first.
        candidates: Vec<String>,
    },
}

/// An unmatched statement line.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconLine {
    /// Statement line id.
    pub id: String,
    /// Booking date.
    pub booked_on: String,
    /// Signed amount, minor units.
    pub amount_minor: i64,
    /// Counterparty or narration.
    pub description: String,
}

fn cash_sides(ledger: &Ledger, account: &str) -> Vec<recon_kit::LedgerSide> {
    use recon_kit::{Direction, LedgerSide};
    ledger
        .posted_entries()
        .iter()
        .flat_map(|entry| {
            entry.lines.iter().filter_map(move |line| {
                if line.account.as_str() != account {
                    return None;
                }
                let direction = match line.side {
                    double_entry::Side::Debit => Direction::Credit,
                    double_entry::Side::Credit => Direction::Debit,
                };
                Some(LedgerSide {
                    entry_id: entry.id.clone(),
                    booked_on: entry.date.clone(),
                    amount: u64::try_from(line.amount.units.max(0)).unwrap_or(0),
                    direction,
                })
            })
        })
        .collect()
}

/// Reconciles `statement` against the cash account's posted lines.
///
/// # Errors
/// Returns the statement-validation message when the balance identity fails.
pub fn reconcile(
    ledger: &Ledger,
    statement: &recon_kit::Statement,
    account: &str,
    window_days: i64,
) -> Result<ReconView, String> {
    statement.validate().map_err(|error| format!("{error:?}"))?;
    let entries = cash_sides(ledger, account);
    let taken_line_ids = std::collections::HashSet::new();
    let taken_entry_ids = std::collections::HashSet::new();

    let exact = recon_kit::match_exact(
        &statement.lines,
        &entries,
        &taken_line_ids,
        &taken_entry_ids,
    );
    let claimed_lines: std::collections::HashSet<String> =
        exact.iter().map(|m| m.line_id.clone()).collect();
    let claimed_entries: std::collections::HashSet<String> =
        exact.iter().map(|m| m.entry_id.clone()).collect();
    let proposals = recon_kit::match_window(
        &statement.lines,
        &entries,
        window_days,
        &claimed_lines,
        &claimed_entries,
    );

    let mut claimed = claimed_lines;
    // Both variants claim their line the same way; only the payload shape
    // differs.
    #[allow(clippy::match_same_arms)]
    for proposal in &proposals {
        let line_id = match proposal {
            recon_kit::Proposal::Match(recon_kit::Match { line_id, .. }) => line_id,
            recon_kit::Proposal::Ambiguous { line_id, .. } => line_id,
        };
        claimed.insert(line_id.clone());
    }
    let unmatched: Vec<ReconLine> = statement
        .lines
        .iter()
        .filter(|line| !claimed.contains(&line.id))
        .map(|line| ReconLine {
            id: line.id.clone(),
            booked_on: line.booked_on.clone(),
            amount_minor: line.direction.signed(line.amount),
            description: line.description.clone(),
        })
        .collect();

    Ok(ReconView {
        currency: statement.currency.clone(),
        opening_balance: statement.opening_balance,
        closing_balance: statement.closing_balance,
        computed_closing: statement.computed_closing(),
        valid: statement.validate().is_ok(),
        exact: exact
            .iter()
            .map(|m| ReconMatch {
                line_id: m.line_id.clone(),
                entry_id: m.entry_id.clone(),
            })
            .collect(),
        proposals: proposals
            .iter()
            .map(|proposal| match proposal {
                recon_kit::Proposal::Match(recon_kit::Match {
                    line_id,
                    entry_id,
                }) => ReconProposal::Match {
                    line_id: line_id.clone(),
                    entry_id: entry_id.clone(),
                },
                recon_kit::Proposal::Ambiguous { line_id, candidates } => {
                    ReconProposal::Ambiguous {
                        line_id: line_id.clone(),
                        candidates: candidates.clone(),
                    }
                }
            })
            .collect(),
        unmatched,
    })
}

/// Loads the snapshot from disk, if present.
///
/// # Errors
/// Returns I/O errors when the file exists but cannot be read.
pub fn load_snapshot(path: &std::path::Path) -> Result<Option<String>, std::io::Error> {
    if path.exists() {
        Ok(Some(std::fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}
