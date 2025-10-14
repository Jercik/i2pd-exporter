use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use clap::Parser;
use log::{error, info, warn};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
#[cfg(unix)]
use tokio::signal::unix::{signal as unix_signal, SignalKind};
use warp::http::HeaderMap;
use warp::Filter;

// --- Helpers ---
// Escape characters in Prometheus label values: backslash, newline, and quote
fn escape_label(s: &str) -> String {
    s.replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}

fn truncate_chars(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        t
    }
}

// Write HELP and TYPE lines together for a metric family
fn help(buf: &mut String, name: &str, help: &str, mtype: &str) {
    use std::fmt::Write as _;
    writeln!(buf, "# HELP {} {}", name, help).ok();
    writeln!(buf, "# TYPE {} {}", name, mtype).ok();
}

// Write a single metric sample, optionally with labels
fn sample(buf: &mut String, name: &str, labels: &[(&str, &str)], value: impl std::fmt::Display) {
    use std::fmt::Write as _;
    if labels.is_empty() {
        writeln!(buf, "{} {}", name, value).ok();
        return;
    }
    write!(buf, "{}{{", name).ok();
    for (i, (k, v)) in labels.iter().enumerate() {
        if i > 0 {
            write!(buf, ",").ok();
        }
        let ev = escape_label(v);
        write!(buf, "{}=\"{}\"", k, ev).ok();
    }
    writeln!(buf, "}} {}", value).ok();
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
    timeout: Duration,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let req = serde_json::json!({
        "id": 1,
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    let resp = client.post(url).json(&req).timeout(timeout).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let snippet = if body.chars().count() > 2048 {
            truncate_chars(&body, 2048)
        } else {
            body.clone()
        };
        return Err(format!("HTTP {} calling {}: body: {}", status, method, snippet).into());
    }
    let text = resp.text().await?;
    // Optional debug logging for RouterInfo body (can be verbose). Avoid logging Authenticate to not leak secrets.
    if std::env::var("DEBUG_I2PCONTROL_BODY").ok().as_deref() == Some("1") && method == "RouterInfo"
    {
        // Truncate to avoid excessive logs
        let snippet = if text.chars().count() > 4096 {
            truncate_chars(&text, 4096)
        } else {
            text.clone()
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
            let snippet = if text.chars().count() > 2048 {
                truncate_chars(&text, 2048)
            } else {
                text.clone()
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
    base_http_timeout: Duration,  // Base timeout from env, used as an upper bound
}

impl AppState {
    // Creates a new AppState instance.
    fn new(
        api_client: reqwest::Client,
        api_url: String,
        password: String,
        base_http_timeout: Duration,
    ) -> Self {
        AppState {
            api_client,
            api_url,
            password,
            token: Mutex::new(None),
            base_http_timeout,
        }
    }

    // Authenticate with the I2PControl JSON-RPC API using the configured password.
    // Stores the obtained token in the AppState's Mutex and returns it.
    async fn authenticate(
        &self,
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let params = serde_json::json!({ "API": 1, "Password": self.password });
        let result: AuthResult = rpc_call(
            &self.api_client,
            &self.api_url,
            "Authenticate",
            params,
            timeout,
        )
        .await?;

        if let Some(token) = result.token {
            {
                let mut guard = self.token.lock().await;
                *guard = Some(token.clone());
            }
            info!("Obtained authentication token from I2PControl");
            return Ok(token);
        }

        Err("Authentication failed: no token received".into())
    }

    // Fetch router information from the I2PControl API.
    // Handles token acquisition and re-authentication if the token expires.
    async fn fetch_router_info(
        &self,
        timeout: Duration,
    ) -> Result<RouterInfoResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut did_retry = false; // Flag to prevent infinite retry loops

        loop {
            // Loop to handle potential re-authentication
            // Get the current token from the mutex
            let current_token = {
                let guard = self.token.lock().await; // Lock the mutex
                guard.clone() // Clone the Option<String>
            }; // Mutex guard is dropped here

            // If no token exists, call authenticate() to get one.
            // If a token exists, use it.
            let token = match current_token {
                Some(tok) => tok,
                None => {
                    info!("No token found, authenticating...");
                    self.authenticate(timeout).await?
                }
            };

            // Build the parameters for the 'RouterInfo' JSON-RPC request.
            // We request specific keys related to router status, bandwidth, network, etc.
            let mut params = serde_json::Map::new();
            for key in &[
                "i2p.router.status",                    // Router status as string "1" or "0"
                "i2p.router.version",                   // Request router version string
                "i2p.router.uptime",                    // Request uptime in milliseconds
                "i2p.router.net.bw.inbound.1s",         // Request inbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.inbound.15s",        // Request inbound bandwidth (15s avg, Bps)
                "i2p.router.net.bw.outbound.1s",        // Request outbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.outbound.15s",       // Request outbound bandwidth (15s avg, Bps)
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
                timeout,
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
                            let mut guard = self.token.lock().await;
                            *guard = None;
                        }
                        let _ = self.authenticate(timeout).await?;
                        did_retry = true;
                        continue;
                    }
                    return Err(err);
                }
            };

            return Ok(data);
        }
    }

    // Fetch metrics using the 'RouterInfo' API method and format them for Prometheus.
    // Handles token acquisition and re-authentication if the token expires.
    async fn fetch_metrics(
        &self,
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let data = self.fetch_router_info(timeout).await?;
        Ok(format_metrics(&data))
    }
}

// Format RouterInfo data as Prometheus metrics output.
fn format_metrics(data: &RouterInfoResult) -> String {
    // Build the Prometheus output
    let mut output = String::with_capacity(1024);

    // Router status (I2PControl returns "1" or "0" as a string)
    if let Some(status) = &data.router_status {
        help(
            &mut output,
            "i2p_router_status",
            "Router status (1 or 0)",
            "gauge",
        );
        let status_value = status.trim().parse::<u64>().unwrap_or(0);
        sample(&mut output, "i2p_router_status", &[], status_value);
    }

    // Router build info (with version label)
    if let Some(version) = &data.router_version {
        help(
            &mut output,
            "i2p_router_build_info",
            "Router build information",
            "gauge",
        );
        sample(
            &mut output,
            "i2p_router_build_info",
            &[("version", version.as_str())],
            1,
        );
    }

    // Metric: Router Uptime (convert ms to seconds)
    if let Some(ms) = data.router_uptime {
        let seconds = (ms as f64) / 1000.0; // Convert ms to s
        help(
            &mut output,
            "i2p_router_uptime_seconds",
            "Router uptime in seconds",
            "gauge",
        );
        sample(
            &mut output,
            "i2p_router_uptime_seconds",
            &[],
            format!("{:.3}", seconds),
        );
    }

    // Metrics: Bandwidth (Inbound/Outbound, 1s/15s windows, Bytes/sec)
    let any_bw = data.bw_inbound_1s.is_some()
        || data.bw_inbound_15s.is_some()
        || data.bw_outbound_1s.is_some()
        || data.bw_outbound_15s.is_some();
    if any_bw {
        help(
            &mut output,
            "i2p_router_net_bw_bytes_per_second",
            "Router bandwidth in bytes/sec",
            "gauge",
        );
    }
    if data.bw_inbound_1s.is_some() || data.bw_inbound_15s.is_some() {
        if let Some(bw) = data.bw_inbound_1s {
            // 1-second average
            sample(
                &mut output,
                "i2p_router_net_bw_bytes_per_second",
                &[("direction", "inbound"), ("window", "1s")],
                bw,
            );
        }
        if let Some(bw) = data.bw_inbound_15s {
            // 15-second average
            sample(
                &mut output,
                "i2p_router_net_bw_bytes_per_second",
                &[("direction", "inbound"), ("window", "15s")],
                bw,
            );
        }
    }
    if data.bw_outbound_1s.is_some() || data.bw_outbound_15s.is_some() {
        // Outbound points (no duplicate HELP/TYPE)
        if let Some(bw) = data.bw_outbound_1s {
            // 1-second average
            sample(
                &mut output,
                "i2p_router_net_bw_bytes_per_second",
                &[("direction", "outbound"), ("window", "1s")],
                bw,
            );
        }
        if let Some(bw) = data.bw_outbound_15s {
            // 15-second average
            sample(
                &mut output,
                "i2p_router_net_bw_bytes_per_second",
                &[("direction", "outbound"), ("window", "15s")],
                bw,
            );
        }
    }

    // Metric: Net Status (IPv4)
    if let Some(status) = data.net_status {
        help(
            &mut output,
            "i2p_router_net_status",
            "IPv4 network status as states (ok, firewalled, unknown, proxy, mesh)",
            "gauge",
        );
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
            sample(
                &mut output,
                "i2p_router_net_status",
                &[("state", state)],
                val,
            );
        }

        // Metric: Net Status Code (raw numeric value)
        help(
            &mut output,
            "i2p_router_net_status_code",
            "IPv4 network status code (0=OK, 1=Firewalled, 2=Unknown, 3=Proxy, 4=Mesh)",
            "gauge",
        );
        sample(&mut output, "i2p_router_net_status_code", &[], status);
    }

    // Metric: Participating Tunnels
    if let Some(count) = data.tunnels_participating {
        help(
            &mut output,
            "i2p_router_tunnels_participating",
            "Number of active participating transit tunnels",
            "gauge",
        );
        sample(&mut output, "i2p_router_tunnels_participating", &[], count);
    }

    // Metric: Tunnels success rate as ratio (0..1)
    if let Some(percent) = data.tunnels_successrate {
        let ratio = (percent / 100.0).clamp(0.0, 1.0);
        help(
            &mut output,
            "i2p_router_tunnels_success_ratio",
            "Tunnel build success rate as a ratio (0..1)",
            "gauge",
        );
        sample(
            &mut output,
            "i2p_router_tunnels_success_ratio",
            &[],
            format!("{:.6}", ratio),
        );
    }

    // Metrics: NetDB Peer Statistics
    if let Some(count) = data.netdb_activepeers {
        help(
            &mut output,
            "i2p_router_netdb_activepeers",
            "Number of active known peers in NetDB",
            "gauge",
        );
        sample(&mut output, "i2p_router_netdb_activepeers", &[], count);
    }
    if let Some(count) = data.netdb_knownpeers {
        help(
            &mut output,
            "i2p_router_netdb_knownpeers",
            "Total number of known peers (RouterInfos) in NetDB",
            "gauge",
        );
        sample(&mut output, "i2p_router_netdb_knownpeers", &[], count);
    }

    // Metrics: Total Network Bytes (received/sent)
    if data.net_total_received_bytes.is_some() || data.net_total_sent_bytes.is_some() {
        help(
            &mut output,
            "i2p_router_net_bytes_total",
            "Total network bytes since router start",
            "counter",
        );
        if let Some(v) = data.net_total_received_bytes {
            sample(
                &mut output,
                "i2p_router_net_bytes_total",
                &[("direction", "inbound")],
                v,
            );
        }
        if let Some(v) = data.net_total_sent_bytes {
            sample(
                &mut output,
                "i2p_router_net_bytes_total",
                &[("direction", "outbound")],
                v,
            );
        }
    }

    // Metric: Exporter build info (info gauge)
    help(
        &mut output,
        "i2pd_exporter_build_info",
        "Exporter build information",
        "gauge",
    );
    // Use CARGO_PKG_VERSION env var set at compile time
    sample(
        &mut output,
        "i2pd_exporter_build_info",
        &[("version", EXPORTER_VERSION)],
        1,
    );

    output
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
    // Allow invalid certs if env set or host is loopback.
    let tls_insecure_env = std::env::var("I2PCONTROL_TLS_INSECURE").ok().as_deref() == Some("1");
    let host_is_loopback = reqwest::Url::parse(&i2p_addr)
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
        .danger_accept_invalid_certs(allow_insecure)
        .timeout(Duration::from_secs(http_timeout))
        .user_agent(format!("i2pcontrol-exporter/{}", EXPORTER_VERSION))
        .build()?;

    let state = Arc::new(AppState::new(
        api_client,
        format!("{}/jsonrpc", i2p_addr.trim_end_matches('/')),
        i2p_password,
        Duration::from_secs(http_timeout),
    ));

    // Attempt initial auth (logs error if fails; will retry on first request anyway)
    if !state.password.is_empty() {
        if let Err(e) = state.authenticate(state.base_http_timeout).await {
            error!("Initial authentication failed: {}", e);
        }
    }

    // Define a small async handler function for /metrics
    async fn metrics_handler(
        st: Arc<AppState>,
        headers: HeaderMap,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let t0 = Instant::now();

        // Compute effective timeout for this scrape based on Prometheus header
        // X-Prometheus-Scrape-Timeout-Seconds is a float seconds budget
        let mut effective_timeout = st.base_http_timeout;
        if let Some(val) = headers.get("X-Prometheus-Scrape-Timeout-Seconds") {
            if let Ok(s) = val.to_str() {
                if let Ok(hdr_secs) = s.parse::<f64>() {
                    // Subtract 0.5s safety margin; clamp to a small minimum
                    let mut hdr_budget = hdr_secs - 0.5;
                    if hdr_budget <= 0.0 {
                        hdr_budget = 0.1; // minimal to avoid zero
                    }
                    let hdr_dur = Duration::from_secs_f64(hdr_budget);
                    if hdr_dur < effective_timeout {
                        effective_timeout = hdr_dur;
                    }
                }
            }
        }

        // Attempt to fetch target metrics within the overall scrape budget
        let (status_code, mut body) = match tokio::time::timeout(
            effective_timeout,
            st.fetch_metrics(effective_timeout),
        )
        .await
        {
            Err(_elapsed) => (warp::http::StatusCode::GATEWAY_TIMEOUT, String::new()),
            Ok(Ok(metrics)) => (warp::http::StatusCode::OK, metrics),
            Ok(Err(err)) => {
                error!("Failed to fetch metrics: {}", err);
                (warp::http::StatusCode::INTERNAL_SERVER_ERROR, String::new())
            }
        };

        // Always append exporter self-metrics: scrape duration
        let scrape_seconds = t0.elapsed().as_secs_f64();
        help(
            &mut body,
            "i2pd_exporter_scrape_duration_seconds",
            "Duration of last scrape",
            "gauge",
        );
        sample(
            &mut body,
            "i2pd_exporter_scrape_duration_seconds",
            &[],
            scrape_seconds,
        );

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
        .and(warp::header::headers_cloned())
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
            tokio::signal::ctrl_c()
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
