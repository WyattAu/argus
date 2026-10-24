//! argus — all-purpose intelligence platform. M1: world-intel feeds
//! (USGS, EONET, SWPC, NVD, news RSS, FIRMS), bus-backed REST + WS surface.

mod api;
mod config;
mod error;
mod feeds;
mod state;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use feedkit::{EonetSource, FirmsSource, NewsSource, NvdSource, SimMarket, SwpcSource, UsgsSource};
use healthkit::HealthStatus;
use tokio::signal::unix::{signal, SignalKind};

use crate::config::ArgusConfig;
use crate::state::AppState;

/// CLI arguments.
#[derive(Debug, Parser)]
#[command(name = "argus", version, about = "All-purpose intelligence platform")]
struct Args {
    /// Path to the TOML config file.
    #[arg(long, default_value = "argus.toml")]
    config: PathBuf,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(&args) {
        eprintln!("argus: fatal: {error}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load(&args.config)?;

    let telemetry = telemetry_init::Telemetry::init(
        telemetry_init::TelemetryConfig::new("argus")
            .version(env!("CARGO_PKG_VERSION"))
            .log_level(cfg.telemetry.level.clone()),
    )?;
    tracing::info!(config = %args.config.display(), "argus starting");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(serve(cfg));
    runtime.shutdown_timeout(Duration::from_secs(5));

    if let Err(error) = telemetry.shutdown() {
        tracing::warn!(%error, "telemetry shutdown failed");
    }
    result
}

async fn serve(cfg: config::ArgusConfig) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(AppState::new(cfg));
    spawn_feeds(&state);
    register_health_checks(&state);
    feeds::bridge_bus(Arc::clone(&state)).await;

    let addr: SocketAddr = format!("{}:{}", state.cfg.server.host, state.cfg.server.port)
        .parse()
        .map_err(|e| format!("invalid bind address: {e}"))?;
    let app = api::router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "argus listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// One enabled feed: name, poll interval, and the staleness budget that
/// marks its health check `Degraded`.
struct FeedSpec {
    name: &'static str,
    interval: Duration,
    budget: Duration,
}

fn enabled_feeds(cfg: &ArgusConfig) -> Vec<FeedSpec> {
    let f = &cfg.feeds;
    let mut specs = Vec::new();
    if f.sim_enabled {
        specs.push(FeedSpec {
            name: "sim",
            interval: Duration::from_millis(f.sim_interval_ms.max(50)),
            budget: Duration::from_millis((f.sim_interval_ms.max(50)).saturating_mul(5)),
        });
    }
    specs.extend(timed_spec("usgs", f.usgs_enabled, f.usgs_interval_secs, 3));
    specs.extend(timed_spec("eonet", f.eonet_enabled, f.eonet_interval_secs, 3));
    specs.extend(timed_spec("swpc", f.swpc_enabled, f.swpc_interval_secs, 3));
    specs.extend(timed_spec("nvd", f.nvd_enabled, f.nvd_interval_secs, 3));
    specs.extend(timed_spec("news", f.news_enabled, f.news_interval_secs, 3));
    specs.extend(timed_spec("firms", f.firms_enabled, f.firms_interval_secs, 3));
    specs
}

/// Timed-feed spec factory: `Degraded` budget is `multiplier * interval`
/// with a 45 s floor.
fn timed_spec(
    name: &'static str,
    enabled: bool,
    secs: u64,
    multiplier: u64,
) -> Option<FeedSpec> {
    if !enabled {
        return None;
    }
    let secs = secs.max(1);
    Some(FeedSpec {
        name,
        interval: Duration::from_secs(secs),
        budget: Duration::from_secs(secs.saturating_mul(multiplier).max(45)),
    })
}

fn spawn_feeds(state: &Arc<AppState>) {
    let f = &state.cfg.feeds;
    for spec in enabled_feeds(&state.cfg) {
        match spec.name {
            "sim" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                SimMarket::new(),
                spec.interval,
            )),
            "usgs" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                UsgsSource::new(),
                spec.interval,
            )),
            "eonet" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                EonetSource::new(),
                spec.interval,
            )),
            "swpc" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                SwpcSource::new(),
                spec.interval,
            )),
            "nvd" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                NvdSource::new(),
                spec.interval,
            )),
            "news" => tokio::spawn(feeds::run_source(
                Arc::clone(state),
                NewsSource::bbc_world(),
                spec.interval,
            )),
            "firms" => {
                let Some(source) = FirmsSource::new(&f.firms_map_key) else {
                    tracing::warn!("firms feed enabled but firms_map_key is empty; skipping");
                    continue;
                };
                tokio::spawn(feeds::run_source(Arc::clone(state), source, spec.interval))
            }
            other => {
                tracing::warn!(feed = other, "unknown feed spec; skipping");
                continue;
            }
        };
    }
}

fn register_health_checks(state: &Arc<AppState>) {
    for spec in enabled_feeds(&state.cfg) {
        let st = Arc::clone(state);
        let name = spec.name;
        let budget_ms = i64::try_from(spec.budget.as_millis()).unwrap_or(i64::MAX);
        state
            .health
            .add_check(format!("feed-{name}"), move || {
                let st = Arc::clone(&st);
                async move {
                    let age = st.ms_since_ok(name, feedkit::now_ms());
                    if age < budget_ms {
                        Ok(HealthStatus::Healthy)
                    } else {
                        Ok(HealthStatus::Degraded)
                    }
                }
            });
    }
}

/// Resolves on ctrl-C or SIGTERM.
async fn shutdown_signal() {
    let terminate = signal(SignalKind::terminate());
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            if let Err(error) = result {
                tracing::warn!(%error, "ctrl-C handler unavailable");
            }
        }
        () = async {
            match terminate {
                Ok(mut stream) => {
                    let _ = stream.recv().await;
                }
                Err(error) => {
                    tracing::warn!(%error, "SIGTERM handler unavailable");
                    std::future::pending::<()>().await;
                }
            }
        } => {}
    }
}
