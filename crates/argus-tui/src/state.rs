//! Shared TUI state: the mirror of the bus the UI renders.

use serde_json::Value;
use std::collections::BTreeMap;

/// Everything the UI draws, updated by the stream task.
#[derive(Debug, Default)]
pub struct TuiState {
    /// Latest quote per symbol, symbol-ordered.
    quotes: BTreeMap<String, f64>,
    /// Rendered observation lines, newest first.
    observation_lines: Vec<String>,
    /// Rendered alert lines, newest first.
    alert_lines: Vec<String>,
    /// Whether the stream looks quiet.
    pub stale: bool,
    last_msg_ms: i64,
}

/// Cap for the observation list.
const OBSERVATION_LINES: usize = 60;
/// Cap for the alert list.
const ALERT_LINES: usize = 40;

impl TuiState {
    /// Empty state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            quotes: BTreeMap::new(),
            observation_lines: Vec::new(),
            alert_lines: Vec::new(),
            stale: true,
            last_msg_ms: 0,
        }
    }

    /// Marks the stream as heard-from.
    pub fn touch(&mut self) {
        self.last_msg_ms = now_ms();
        self.stale = false;
    }

    /// Marks the stream dead (close or watchdog).
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Ingests one stream envelope (`topic` + `payload` JSON object).
    pub fn ingest(&mut self, topic: &str, payload: &serde_json::Value) {
        self.touch();
        match payload.get("type").and_then(Value::as_str) {
            Some("quote") => {
                if let (Some(symbol), Some(price)) = (
                    payload.get("symbol").and_then(Value::as_str),
                    payload.get("price").and_then(Value::as_f64),
                ) {
                    self.quotes.insert(symbol.to_string(), price);
                }
            }
            Some("observation") => {
                let kind = payload
                    .get("type")
                    .and_then(|_| topic.split('.').nth(2))
                    .unwrap_or("event");
                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(untitled)");
                let id = payload
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                push_front(&mut self.observation_lines, format!("[{kind}] {title}"), id, OBSERVATION_LINES);
            }
            Some("alert") => {
                let rule = payload
                    .get("rule_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("rule");
                let message = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                push_front(
                    &mut self.alert_lines,
                    format!("[{rule}] {message}"),
                    &format!("{rule}:{message}"),
                    ALERT_LINES,
                );
            }
            _ => {}
        }
        if self.observation_lines.len() > OBSERVATION_LINES {
            self.observation_lines.truncate(OBSERVATION_LINES);
        }
    }

    /// Seeds observations from the REST endpoint's JSON envelope.
    pub fn seed_observations(&mut self, data: &[serde_json::Value]) {
        let mut lines = Vec::new();
        for payload in data {
            let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("event");
            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            lines.push(format!("[{kind}] {title}"));
        }
        self.observation_lines = lines;
    }

    /// Compact quote strip for the header.
    #[must_use]
    pub fn quote_lines(&self) -> String {
        let mut parts = Vec::new();
        for (symbol, price) in &self.quotes {
            let rendered = if *price >= 100.0 {
                format!("{symbol} {:.0}", *price / 1.0)
            } else {
                format!("{symbol} {price:.4}")
            };
            parts.push(rendered);
        }
        if parts.is_empty() {
            "no quotes yet".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// Observation lines for the panel.
    #[must_use]
    pub fn observation_lines(&self) -> &[String] {
        &self.observation_lines
    }

    /// Alert lines for the panel.
    #[must_use]
    pub fn alert_lines(&self) -> &[String] {
        &self.alert_lines
    }
}

fn push_front(list: &mut Vec<String>, line: String, dedupe_key: &str, cap: usize) {
    if list.iter().any(|existing| existing.contains(dedupe_key)) && !dedupe_key.is_empty() {
        return;
    }
    list.insert(0, line);
    while list.len() > cap {
        list.pop();
    }
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(i64::MAX)
}
