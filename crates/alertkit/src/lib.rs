//! alertkit — rule-based alerting for the argus platform.
//!
//! [`Engine`] evaluates configured [`Rule`]s against platform events and
//! emits [`feedkit::Alert`]s, with per-rule+entity cooldowns so a storm
//! fires once, not a thousand times. [`TelegramSink`] delivers alerts to a
//! Telegram chat; the platform bus carries every alert on
//! `alerts.rule.<id>` for any other consumer.

#![forbid(unsafe_code)]

mod engine;
mod rule;
mod telegram;

pub use engine::Engine;
pub use feedkit::Alert;
pub use rule::{Condition, Rule};
pub use telegram::TelegramSink;
