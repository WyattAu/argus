//! argus — all-purpose intelligence platform. M0 skeleton: bus-backed feeds,
//! REST + WebSocket surface, estate-crate plumbing end to end.

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
use healthkit::HealthStatus;
use state::{AppState, SourceId};
use tokio::signal::unix::{signal, SignalKind};

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

fn spawn_feeds(state: &Arc<AppState>) {
    let feeds = &state.cfg.feeds;
    if feeds.sim_enabled {
        let sim_interval = Duration::from_millis(feeds.sim_interval_ms.max(50));
        tokio::spawn(feeds::run_source(
            Arc::clone(state),
            feedkit::SimMarket::new(),
            sim_interval,
        ));
    }
    if feeds.usgs_enabled {
        let usgs_interval = Duration::from_secs(feeds.usgs_interval_secs.max(5));
        tokio::spawn(feeds::run_source(
            Arc::clone(state),
            feedkit::UsgsSource::new(),
            usgs_interval,
        ));
    }
}

fn register_health_checks(state: &Arc<AppState>) {
    let freshness = |st: &Arc<AppState>, source: SourceId, budget_ms: i64| {
        let st = Arc::clone(st);
        move || {
            let st = Arc::clone(&st);
            async move {
                let age = st.ms_since_ok(source, feedkit::now_ms());
                if age < budget_ms {
                    Ok(HealthStatus::Healthy)
                } else {
                    Ok(HealthStatus::Degraded)
                }
            }
        }
    };

    let sim_budget = i64::try_from(state.cfg.feeds.sim_interval_ms.max(50) * 5)
        .unwrap_or(i64::MAX);
    state
        .health
        .add_check("feed-sim", freshness(state, SourceId::Sim, sim_budget));

    let usgs_budget =
        i64::try_from(state.cfg.feeds.usgs_interval_secs.max(5).saturating_mul(3).saturating_mul(1000))
            .unwrap_or(i64::MAX);
    state
        .health
        .add_check("feed-usgs", freshness(state, SourceId::Usgs, usgs_budget));
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
