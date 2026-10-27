//! Rule evaluation with per-rule+entity cooldowns.

use std::collections::HashMap;

use feedkit::{Alert, Observation, Quote};

use crate::rule::{Condition, Rule};

/// Evaluates rules against events. Not `Sync` — the platform owns it behind
/// a lock and calls it from the bus bridge.
#[derive(Debug)]
pub struct Engine {
    rules: Vec<Rule>,
    last_fired: HashMap<String, i64>,
}

/// A rule that matched, pre-computed before touching cooldown state.
struct Matched {
    rule_id: String,
    rule_name: String,
    cooldown_secs: u64,
    message: String,
    entity: String,
}

impl Engine {
    /// Engine with the configured rules.
    #[must_use]
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            last_fired: HashMap::new(),
        }
    }

    /// Number of configured rules.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Evaluates all rules against one observation.
    #[must_use]
    pub fn evaluate_observation(&mut self, o: &Observation, now_ms: i64) -> Vec<Alert> {
        let matched: Vec<Matched> = self
            .rules
            .iter()
            .filter_map(|rule| observation_match(rule, o))
            .collect();
        matched
            .into_iter()
            .filter_map(|m| {
                self.fire_allowed(&m.rule_id, &m.entity, m.cooldown_secs, now_ms)
                    .then_some(Alert {
                        rule_id: m.rule_id,
                        rule_name: m.rule_name,
                        message: m.message,
                        entity: m.entity,
                        ts_ms: now_ms,
                    })
            })
            .collect()
    }

    /// Evaluates all rules against one quote.
    #[must_use]
    pub fn evaluate_quote(&mut self, q: &Quote, now_ms: i64) -> Vec<Alert> {
        let matched: Vec<Matched> = self
            .rules
            .iter()
            .filter_map(|rule| quote_match(rule, q))
            .collect();
        matched
            .into_iter()
            .filter_map(|m| {
                self.fire_allowed(&m.rule_id, &m.entity, m.cooldown_secs, now_ms)
                    .then_some(Alert {
                        rule_id: m.rule_id,
                        rule_name: m.rule_name,
                        message: m.message,
                        entity: m.entity,
                        ts_ms: now_ms,
                    })
            })
            .collect()
    }

    fn fire_allowed(
        &mut self,
        rule_id: &str,
        entity: &str,
        cooldown_secs: u64,
        now_ms: i64,
    ) -> bool {
        let key = format!("{rule_id}:{entity}");
        let cooldown_ms = i64::try_from(cooldown_secs.saturating_mul(1000)).unwrap_or(i64::MAX);
        match self.last_fired.get(&key) {
            Some(last) if now_ms - *last < cooldown_ms => false,
            _ => {
                self.last_fired.insert(key, now_ms);
                // Bound the map; stale entries only cost a re-fire.
                if self.last_fired.len() > 10_000 {
                    self.last_fired.clear();
                }
                true
            }
        }
    }
}

fn observation_match(rule: &Rule, o: &Observation) -> Option<Matched> {
    if let Some(source) = &rule.source {
        if source != &o.source {
            return None;
        }
    }
    if let Some(kind) = &rule.kind {
        if !kind.eq_ignore_ascii_case(feedkit::kind_slug(o.kind)) {
            return None;
        }
    }
    let Condition::MagnitudeAtLeast { value } = &rule.condition else {
        return None;
    };
    let magnitude = o.magnitude?;
    if magnitude < *value {
        return None;
    }
    let entity = format!("event:{}:{}", o.source, o.id);
    Some(Matched {
        rule_id: rule.id.clone(),
        rule_name: display_name(rule),
        cooldown_secs: rule.cooldown_secs,
        message: format!("{} (magnitude {magnitude:.1})", o.title),
        entity,
    })
}

fn quote_match(rule: &Rule, q: &Quote) -> Option<Matched> {
    if let Some(symbol) = &rule.symbol {
        if !symbol.eq_ignore_ascii_case(&q.symbol) {
            return None;
        }
    }
    let fires = match &rule.condition {
        Condition::PriceAbove { value } => q.price > *value,
        Condition::PriceBelow { value } => q.price < *value,
        Condition::MagnitudeAtLeast { .. } => false,
    };
    if !fires {
        return None;
    }
    let entity = format!("symbol:{}", q.symbol.to_uppercase());
    Some(Matched {
        rule_id: rule.id.clone(),
        rule_name: display_name(rule),
        cooldown_secs: rule.cooldown_secs,
        message: format!("{} at {:.2}", q.symbol, q.price),
        entity,
    })
}

fn display_name(rule: &Rule) -> String {
    if rule.name.is_empty() {
        rule.id.clone()
    } else {
        rule.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::Engine;
    use crate::rule::{Condition, Rule};
    use feedkit::{Observation, ObservationKind, Quote};

    fn quake_obs(magnitude: Option<f64>) -> Observation {
        Observation {
            id: "usgs-test-1".to_string(),
            source: "usgs".to_string(),
            kind: ObservationKind::Quake,
            title: "M 5.0 - testquake".to_string(),
            place: None,
            lat: None,
            lon: None,
            magnitude,
            url: None,
            occurred_at_ms: 1_000,
            fetched_at_ms: 1_000,
            license: "test".to_string(),
        }
    }

    fn rule() -> Rule {
        Rule {
            id: "quake-major".to_string(),
            name: "Major quake".to_string(),
            source: Some("usgs".to_string()),
            kind: Some("quake".to_string()),
            symbol: None,
            condition: Condition::MagnitudeAtLeast { value: 4.5 },
            cooldown_secs: 900,
        }
    }

    #[test]
    fn fires_on_threshold_and_respects_cooldown() {
        let mut engine = Engine::new(vec![rule()]);
        let first = engine.evaluate_observation(&quake_obs(Some(5.2)), 5_000);
        assert_eq!(first.len(), 1);
        assert_eq!(first.first().map(|a| a.rule_id.as_str()), Some("quake-major"));
        // Same entity inside the cooldown window: suppressed.
        assert!(engine.evaluate_observation(&quake_obs(Some(6.1)), 10_000).is_empty());
        // Past the cooldown: fires again.
        assert_eq!(engine.evaluate_observation(&quake_obs(Some(6.1)), 1_000_000).len(), 1);
    }

    #[test]
    fn no_fire_below_threshold_or_missing_magnitude() {
        let mut engine = Engine::new(vec![rule()]);
        assert!(engine.evaluate_observation(&quake_obs(Some(3.0)), 1_000).is_empty());
        assert!(engine.evaluate_observation(&quake_obs(None), 1_000).is_empty());
    }

    #[test]
    fn source_filter_blocks_other_feeds() {
        let mut engine = Engine::new(vec![rule()]);
        let mut other = quake_obs(Some(6.0));
        other.source = "eonet".to_string();
        assert!(engine.evaluate_observation(&other, 1_000).is_empty());
    }

    #[test]
    fn price_rules_fire_on_the_right_side() {
        let above = Rule {
            id: "btc-high".to_string(),
            name: String::new(),
            source: Some("binance".to_string()),
            kind: None,
            symbol: Some("BTCUSDT".to_string()),
            condition: Condition::PriceAbove { value: 90_000.0 },
            cooldown_secs: 60,
        };
        let mut engine = Engine::new(vec![above]);
        let make = |price: f64| Quote {
            symbol: "BTCUSDT".to_string(),
            price,
            ts_ms: 1_000,
        };
        assert_eq!(engine.evaluate_quote(&make(95_000.0), 1_000).len(), 1);
        let quiet = make(80_000.0);
        assert!(engine.evaluate_quote(&quiet, 2_000).is_empty());
    }
}
