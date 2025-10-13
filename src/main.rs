use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::Parser;
use log::{error, info, warn};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
#[cfg(unix)]
use tokio::signal::unix::{signal as unix_signal, SignalKind};
use warp::Filter;

// --- Helpers ---
// Escape characters in Prometheus label values: backslash, newline, and quote
fn escape_label(s: &str) -> String {
    s.replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}

// Compile-time exporter version string
const EXPORTER_VERSION: &str = env!("CARGO_PKG_VERSION");

// --- CLI Arguments ---

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)] // Automatically uses version from Cargo.toml
struct Cli {}

// --- I2PControl API Response Structures ---

// Represents an error in a JSON-RPC response
#[derive(Debug, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

// Exact-one-of JSON-RPC outcome
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RpcOutcome<T> {
    Ok { result: T },
    Err { error: RpcError },
}

// Generic JSON-RPC call helper
async fn rpc_call<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let req = serde_json::json!({
        "id": 1,
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    let resp = client.post(url).json(&req).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let snippet = if body.len() > 2048 {
            &body[..2048]
        } else {
            &body
        };
        return Err(format!("HTTP {} calling {}: body: {}", status, method, snippet).into());
    }
    let text = resp.text().await?;
    // Optional debug logging for RouterInfo body (can be verbose). Avoid logging Authenticate to not leak secrets.
    if std::env::var("DEBUG_I2PCONTROL_BODY").ok().as_deref() == Some("1") && method == "RouterInfo"
    {
        // Truncate to avoid excessive logs
        let snippet = if text.len() > 4096 {
            &text[..4096]
        } else {
            &text
        };
        log::debug!("{} response body: {}", method, snippet);
    }
    let parsed: Result<RpcOutcome<T>, _> = serde_json::from_str(&text);
    match parsed {
        Ok(RpcOutcome::Ok { result }) => Ok(result),
        Ok(RpcOutcome::Err { error }) => {
            Err(format!("{} error {}: {}", method, error.code, error.message).into())
        }
        Err(e) => {
            let snippet = if text.len() > 2048 {
                &text[..2048]
            } else {
                &text
            };
            Err(format!(
                "error decoding response body for {}: {}; body: {}",
                method, e, snippet
            )
            .into())
        }
    }
}

// Result structure for the 'Authenticate' method
#[derive(Debug, Deserialize, Default)]
struct AuthResult {
    #[serde(rename = "Token")]
    token: Option<String>,
}

// Result structure for the 'RouterInfo' method, containing various metrics
#[derive(Debug, Deserialize, Default)]
struct RouterInfoResult {
    #[serde(rename = "i2p.router.status")]
    router_status: Option<String>,
    #[serde(rename = "i2p.router.version")]
    router_version: Option<String>,
    #[serde(rename = "i2p.router.uptime")]
    router_uptime: Option<u64>,
    #[serde(rename = "i2p.router.net.bw.inbound.1s")]
    bw_inbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.inbound.15s")]
    bw_inbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.1s")]
    bw_outbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.15s")]
    bw_outbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.status")]
    // IPv4 network status code: 0=OK, 1=Firewalled, 2=Unknown, 3=Proxy, 4=Mesh
    net_status: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.participating")]
    tunnels_participating: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.successrate")]
    tunnels_successrate: Option<f64>,
    #[serde(rename = "i2p.router.netdb.activepeers")]
    netdb_activepeers: Option<u64>,
    #[serde(rename = "i2p.router.netdb.knownpeers")]
    netdb_knownpeers: Option<u64>,
    #[serde(rename = "i2p.router.net.total.received.bytes")]
    net_total_received_bytes: Option<f64>,
    #[serde(rename = "i2p.router.net.total.sent.bytes")]
    net_total_sent_bytes: Option<f64>,
}

// --- Application State ---

// Holds shared state for the application, including the API client,
// configuration, and the authentication token (protected by a Mutex).
struct AppState {
    api_client: reqwest::Client,  // HTTP client for making API requests
    api_url: String,              // Full URL for the I2PControl JSON-RPC endpoint
    password: String,             // Password for the I2PControl API
    token: Mutex<Option<String>>, // Current authentication token (None if not authenticated)
    scrapes_total: AtomicU64,     // Total number of scrapes since start
}

impl AppState {
    // Creates a new AppState instance.
    fn new(api_client: reqwest::Client, api_url: String, password: String) -> Self {
        AppState {
            api_client,
            api_url,
            password,
            token: Mutex::new(None),
            scrapes_total: AtomicU64::new(0),
        }
    }

    // Authenticate with the I2PControl JSON-RPC API using the configured password.
    // Stores the obtained token in the AppState's Mutex and returns it.
    async fn authenticate(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let params = serde_json::json!({ "API": 1, "Password": self.password });
        let result: AuthResult =
            rpc_call(&self.api_client, &self.api_url, "Authenticate", params).await?;

        if let Some(token) = result.token {
            {
                let mut guard = self.token.lock().unwrap();
                *guard = Some(token.clone());
            }
            info!("Obtained authentication token from I2PControl");
            return Ok(token);
        }

        Err("Authentication failed: no token received".into())
    }

    // Fetch metrics using the 'RouterInfo' API method and format them for Prometheus.
    // Handles token acquisition and re-authentication if the token expires.
    async fn fetch_metrics(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut did_retry = false; // Flag to prevent infinite retry loops

        loop {
            // Loop to handle potential re-authentication
            // Get the current token from the mutex
            let current_token = {
                let guard = self.token.lock().unwrap(); // Lock the mutex
                guard.clone() // Clone the Option<String>
            }; // Mutex guard is dropped here

            // If no token exists, call authenticate() to get one.
            // If a token exists, use it.
            let token = match current_token {
                Some(tok) => tok,
                None => {
                    info!("No token found, authenticating...");
                    self.authenticate().await?
                }
            };

            // Build the parameters for the 'RouterInfo' JSON-RPC request.
            // We request specific keys related to router status, bandwidth, network, etc.
            let mut params = serde_json::Map::new();
            for key in &[
                "i2p.router.status",  // Request router status string (e.g., "OK", "Testing")
                "i2p.router.version", // Request router version string
                "i2p.router.uptime",  // Request uptime in milliseconds
                "i2p.router.net.bw.inbound.1s", // Request inbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.inbound.15s", // Request inbound bandwidth (15s avg, Bps)
                "i2p.router.net.bw.outbound.1s", // Request outbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.outbound.15s", // Request outbound bandwidth (15s avg, Bps)
                "i2p.router.net.status", // Request IPv4 network status code (0 OK, 1 Firewalled, 2 Unknown, 3 Proxy, 4 Mesh)
                "i2p.router.net.tunnels.participating", // Request participating tunnel count (0 or 1 likely)
                "i2p.router.net.tunnels.successrate", // Request tunnel success rate (percent integer)
                "i2p.router.netdb.activepeers",       // Request active peer count (floodfills)
                "i2p.router.netdb.knownpeers", // Request known peer count (total RouterInfos)
                "i2p.router.net.total.received.bytes", // Request total received bytes
                "i2p.router.net.total.sent.bytes", // Request total sent bytes
            ] {
                // Value::Null indicates we want the value for this key
                params.insert((*key).to_string(), Value::Null);
            }
            // Include the authentication token in the parameters
            params.insert("Token".to_string(), Value::String(token.clone()));

            // Perform JSON-RPC call, handle token expiry with one retry
            let data = match rpc_call::<RouterInfoResult>(
                &self.api_client,
                &self.api_url,
                "RouterInfo",
                Value::Object(params),
            )
            .await
            {
                Ok(data) => data,
                Err(err) => {
                    let msg = err.to_string();
                    let is_token_err =
                        msg.contains("-32002") || msg.contains("-32003") || msg.contains("-32004");
                    if is_token_err && !did_retry {
                        warn!("Token error, re-authenticating...: {}", msg);
                        {
                            let mut guard = self.token.lock().unwrap();
                            *guard = None;
                        }
                        let _ = self.authenticate().await?;
                        did_retry = true;
                        continue;
                    }
                    return Err(err);
                }
            };

            // Build the Prometheus output
            let mut output = String::with_capacity(1024);
            use std::fmt::Write as _;

            // Router status (I2PControl returns "1" or "0" as a string)
            if let Some(status) = &data.router_status {
                writeln!(output, "# HELP i2p_router_status Router status numeric").ok();
                writeln!(output, "# TYPE i2p_router_status gauge").ok();
                let status_value = status.trim().parse::<u64>().unwrap_or(0);
                writeln!(output, "i2p_router_status {}", status_value).ok();
            }

            // Router build info (with version label)
            if let Some(version) = &data.router_version {
                writeln!(
                    output,
                    "# HELP i2p_router_build_info Router build information"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_build_info gauge").ok();
                writeln!(
                    output,
                    r#"i2p_router_build_info{{version="{}"}} 1"#,
                    escape_label(version)
                )
                .ok();
            }

            // Metric: Router Uptime (convert ms to seconds)
            if let Some(ms) = data.router_uptime {
                let seconds = (ms as f64) / 1000.0; // Convert ms to s
                writeln!(
                    output,
                    "# HELP i2p_router_uptime_seconds Router uptime in seconds"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_uptime_seconds gauge").ok();
                writeln!(output, "i2p_router_uptime_seconds {:.3}", seconds).ok();
                // Format to 3 decimal places
            }

            // Metrics: Bandwidth (Inbound/Outbound, 1s/15s windows, Bytes/sec)
            let any_bw = data.bw_inbound_1s.is_some()
                || data.bw_inbound_15s.is_some()
                || data.bw_outbound_1s.is_some()
                || data.bw_outbound_15s.is_some();
            if any_bw {
                writeln!(
                    output,
                    "# HELP i2p_router_net_bw_bytes_per_second Router bandwidth in bytes/sec"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_net_bw_bytes_per_second gauge").ok();
            }
            if data.bw_inbound_1s.is_some() || data.bw_inbound_15s.is_some() {
                if let Some(bw) = data.bw_inbound_1s {
                    // 1-second average
                    writeln!(
                        output,
                        r#"i2p_router_net_bw_bytes_per_second{{direction="inbound",window="1s"}} {}"#,
                        bw
                    )
                    .ok();
                }
                if let Some(bw) = data.bw_inbound_15s {
                    // 15-second average
                    writeln!(
                        output,
                        r#"i2p_router_net_bw_bytes_per_second{{direction="inbound",window="15s"}} {}"#,
                        bw
                    )
                    .ok();
                }
            }
            if data.bw_outbound_1s.is_some() || data.bw_outbound_15s.is_some() {
                // Outbound points (no duplicate HELP/TYPE)
                if let Some(bw) = data.bw_outbound_1s {
                    // 1-second average
                    writeln!(
                        output,
                        r#"i2p_router_net_bw_bytes_per_second{{direction="outbound",window="1s"}} {}"#,
                        bw
                    )
                    .ok();
                }
                if let Some(bw) = data.bw_outbound_15s {
                    // 15-second average
                    writeln!(
                        output,
                        r#"i2p_router_net_bw_bytes_per_second{{direction="outbound",window="15s"}} {}"#,
                        bw
                    )
                    .ok();
                }
            }

            // Metric: Net Status (IPv4)
            if let Some(status) = data.net_status {
                writeln!(
                    output,
                    "# HELP i2p_router_net_status IPv4 network status as states (ok, firewalled, unknown, proxy, mesh)"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_net_status gauge").ok();

                let active = match status {
                    0 => "ok",
                    1 => "firewalled",
                    2 => "unknown",
                    3 => "proxy",
                    4 => "mesh",
                    _ => "unknown",
                };
                for state in ["ok", "firewalled", "unknown", "proxy", "mesh"].iter() {
                    let val = if *state == active { 1 } else { 0 };
                    writeln!(
                        output,
                        r#"i2p_router_net_status{{state="{}"}} {}"#,
                        state, val
                    )
                    .ok();
                }
            }

            // Metric: Participating Tunnels
            if let Some(count) = data.tunnels_participating {
                writeln!(output, "# HELP i2p_router_tunnels_participating Number of active participating transit tunnels").ok();
                writeln!(output, "# TYPE i2p_router_tunnels_participating gauge").ok();
                writeln!(output, "i2p_router_tunnels_participating {}", count).ok();
            }

            // Metric: Tunnels success rate as ratio (0..1)
            if let Some(percent) = data.tunnels_successrate {
                let ratio = (percent / 100.0).clamp(0.0, 1.0);
                writeln!(output, "# HELP i2p_router_tunnels_success_ratio Tunnel build success rate as a ratio (0..1)").ok();
                writeln!(output, "# TYPE i2p_router_tunnels_success_ratio gauge").ok();
                writeln!(output, "i2p_router_tunnels_success_ratio {:.6}", ratio).ok();
            }

            // Metrics: NetDB Peer Statistics
            if let Some(count) = data.netdb_activepeers {
                writeln!(
                    output,
                    "# HELP i2p_router_netdb_activepeers Number of active known peers in NetDB"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_netdb_activepeers gauge").ok();
                writeln!(output, "i2p_router_netdb_activepeers {}", count).ok();
            }
            if let Some(count) = data.netdb_knownpeers {
                writeln!(output, "# HELP i2p_router_netdb_knownpeers Total number of known peers (RouterInfos) in NetDB").ok();
                writeln!(output, "# TYPE i2p_router_netdb_knownpeers gauge").ok();
                writeln!(output, "i2p_router_netdb_knownpeers {}", count).ok();
            }

            // Metrics: Total Network Bytes (received/sent)
            if data.net_total_received_bytes.is_some() || data.net_total_sent_bytes.is_some() {
                writeln!(
                    output,
                    "# HELP i2p_router_net_bytes_total Total network bytes since router start"
                )
                .ok();
                writeln!(output, "# TYPE i2p_router_net_bytes_total counter").ok();

                if let Some(v) = data.net_total_received_bytes {
                    writeln!(
                        output,
                        r#"i2p_router_net_bytes_total{{direction="received"}} {}"#,
                        v
                    )
                    .ok();
                }
                if let Some(v) = data.net_total_sent_bytes {
                    writeln!(
                        output,
                        r#"i2p_router_net_bytes_total{{direction="sent"}} {}"#,
                        v
                    )
                    .ok();
                }
            }

            // Metric: Exporter build info (info gauge)
            writeln!(
                output,
                "# HELP i2pd_exporter_build_info Exporter build information"
            )
            .ok();
            writeln!(output, "# TYPE i2pd_exporter_build_info gauge").ok();
            // Use CARGO_PKG_VERSION env var set at compile time
            writeln!(
                output,
                r#"i2pd_exporter_build_info{{version="{}"}} 1"#,
                escape_label(EXPORTER_VERSION)
            )
            .ok();

            return Ok(output);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse command-line arguments (handles --version automatically)
    let _cli = Cli::parse();

    env_logger::init();

    // Configuration
    let i2p_addr = std::env::var("I2PCONTROL_ADDRESS")
        .unwrap_or_else(|_| "https://127.0.0.1:7650".to_string());
    let i2p_password =
        std::env::var("I2PCONTROL_PASSWORD").unwrap_or_else(|_| "itoopie".to_string());
    let listen_addr =
        std::env::var("METRICS_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9600".to_string());
    let http_timeout = std::env::var("HTTP_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .unwrap_or(60);
    let listen_addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!(
        "Starting I2PControl exporter on {} (target: {})",
        listen_addr, i2p_addr
    );

    // Build an HTTP client for the I2PControl API
    // We accept invalid certs because i2pd uses a self-signed certificate for I2PControl over HTTPS.
    let api_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Allow self-signed certs
        .timeout(Duration::from_secs(http_timeout))
        .build()?;

    let state = Arc::new(AppState::new(
        api_client,
        format!("{}/jsonrpc", i2p_addr.trim_end_matches('/')),
        i2p_password,
    ));

    // Attempt initial auth (logs error if fails; will retry on first request anyway)
    if !state.password.is_empty() {
        if let Err(e) = state.authenticate().await {
            error!("Initial authentication failed: {}", e);
        }
    }

    // Define a small async handler function for /metrics
    async fn metrics_handler(st: Arc<AppState>) -> Result<impl warp::Reply, warp::Rejection> {
        let t0 = Instant::now();

        // Attempt to fetch target metrics
        let (status_code, mut body) = match st.fetch_metrics().await {
            Ok(metrics) => (warp::http::StatusCode::OK, metrics),
            Err(err) => {
                error!("Failed to fetch metrics: {}", err);
                (warp::http::StatusCode::INTERNAL_SERVER_ERROR, String::new())
            }
        };

        // Always append exporter self-metrics: scrape duration and totals
        use std::fmt::Write as _;
        let scrape_seconds = t0.elapsed().as_secs_f64();
        writeln!(
            body,
            "# HELP i2pd_exporter_scrape_duration_seconds Duration of last scrape"
        )
        .ok();
        writeln!(body, "# TYPE i2pd_exporter_scrape_duration_seconds gauge").ok();
        writeln!(
            body,
            "i2pd_exporter_scrape_duration_seconds {}",
            scrape_seconds
        )
        .ok();

        // Increment and expose a total scrapes counter
        let total = st.scrapes_total.fetch_add(1, Ordering::Relaxed) + 1;
        writeln!(
            body,
            "# HELP i2pd_exporter_scrapes_total Total scrapes since exporter start"
        )
        .ok();
        writeln!(body, "# TYPE i2pd_exporter_scrapes_total counter").ok();
        writeln!(body, "i2pd_exporter_scrapes_total {}", total).ok();

        let reply = warp::reply::with_status(body, status_code);
        let reply = warp::reply::with_header(
            reply,
            "Content-Type",
            "text/plain; version=0.0.4; charset=utf-8",
        );
        let reply = warp::reply::with_header(reply, "Cache-Control", "no-store");
        Ok(reply)
    }

    // Warp filter for GET /metrics
    let route_metrics = warp::path("metrics")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::any().map(move || state.clone()))
        .and_then(metrics_handler);

    // Fallback 404 only for root path; avoid matching subpaths
    let route_404 = warp::path::end().and(
        warp::any()
            .map(|| warp::reply::with_status("Not Found", warp::http::StatusCode::NOT_FOUND)),
    );

    // Combine
    let routes = route_metrics.or(route_404);

    info!("Listening on http://{}", listen_addr);
    // Prepare a shutdown signal future that resolves on Ctrl‑C (SIGINT) or SIGTERM.
    let shutdown_signal = async {
        #[cfg(unix)]
        {
            let mut sigterm =
                unix_signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
            let mut sigint =
                unix_signal(SignalKind::interrupt()).expect("Failed to install SIGINT handler");
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM – initiating graceful shutdown");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT (Ctrl+C) – initiating graceful shutdown");
                }
            }
        }
        #[cfg(not(unix))]
        {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
            info!("Received Ctrl+C – initiating graceful shutdown");
        }
    };

    // Start the Warp server with graceful‑shutdown support
    let (_addr, server) =
        warp::serve(routes).bind_with_graceful_shutdown(listen_addr, shutdown_signal);

    server.await;

    Ok(())
}
