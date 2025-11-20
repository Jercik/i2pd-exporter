use std::sync::Arc;

use clap::Parser;
use log::{error, info, warn};

// Module declarations
mod config;
mod i2pcontrol;
mod metrics;
mod server;
pub mod version;

// Import types we need
use config::{Cli, Config};
use i2pcontrol::I2pControlClient;

// Exporter version available as `version::VERSION`

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse CLI + env into Config (handles --version automatically)
    let cli = Cli::parse();
    let cfg = Config::try_from(cli)?;

    env_logger::init();

    // Configuration
    info!(
        "Starting I2PControl exporter on {} (target: {})",
        cfg.listen_addr, cfg.i2p_addr
    );

    // Build an HTTP client for the I2PControl API
    // Allow invalid certs if env set or host is loopback.
    let tls_insecure_env = cfg.tls_insecure;
    let host_is_loopback = reqwest::Url::parse(&cfg.i2p_addr)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .map(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .map(|ip| ip.is_loopback())
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    let allow_insecure = tls_insecure_env || host_is_loopback;

    if tls_insecure_env {
        warn!("I2PCONTROL_TLS_INSECURE=1 set; accepting invalid TLS certificates");
    } else if host_is_loopback {
        info!("Loopback target detected; allowing self-signed certificate");
    }

    let api_client = reqwest::Client::builder()
        .http1_only()
        .danger_accept_invalid_certs(allow_insecure)
        .user_agent(format!("i2pd-exporter/{}", version::VERSION))
        .build()?;

    let state = Arc::new(I2pControlClient::new(
        api_client,
        format!("{}/jsonrpc", cfg.i2p_addr.trim_end_matches('/')),
        cfg.i2p_password,
        cfg.max_scrape_timeout,
    ));

    // Optional quick initial auth so startup doesn't stall; will retry on first scrape.
    if !state.password.is_empty() {
        let quick = std::time::Duration::from_secs(5);
        if let Err(e) = state.authenticate(quick).await {
            error!(
                "Initial authentication failed (will retry on scrape): {}",
                e
            );
        }
    }

    // Build routes via server module
    let routes = server::routes(state.clone());

    info!("Listening on http://{}", cfg.listen_addr);
    // Start the Warp server (simple run; graceful shutdown not available in this resolved Warp)
    warp::serve(routes).run(cfg.listen_addr).await;

    Ok(())
}
