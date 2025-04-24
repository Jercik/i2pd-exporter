use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;

use clap::Parser;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;
use log::{info, warn, error};
use warp::Filter;
use tokio::signal;
#[cfg(unix)]
use tokio::signal::unix::{signal as unix_signal, SignalKind};

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

// Generic JSON-RPC response wrapper, handling optional result or error
#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    #[serde(default)]
    result: Option<T>,
    #[serde(default)]
    error: Option<RpcError>,
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
    router_uptime: Option<Value>,
    #[serde(rename = "i2p.router.net.bw.inbound.1s")]
    bw_inbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.inbound.15s")]
    bw_inbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.1s")]
    bw_outbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.15s")]
    bw_outbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.status")]
    net_status: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.participating")]
    tunnels_participating: Option<u64>,
    #[serde(rename = "i2p.router.netdb.activepeers")]
    netdb_activepeers: Option<u64>,
    #[serde(rename = "i2p.router.netdb.knownpeers")]
    netdb_knownpeers: Option<u64>,
}

// --- Application State ---

// Holds shared state for the application, including the API client,
// configuration, and the authentication token (protected by a Mutex).
struct AppState {
    api_client: reqwest::Client,  // HTTP client for making API requests
    api_url: String,              // Full URL for the I2PControl JSON-RPC endpoint
    password: String,             // Password for the I2PControl API
    token: Mutex<Option<String>>, // Current authentication token (None if not authenticated)
}

impl AppState {
    // Creates a new AppState instance.
    fn new(
        api_client: reqwest::Client,
        api_url: String,
        password: String,
    ) -> Self {
        AppState {
            api_client,
            api_url,
            password,
            token: Mutex::new(None),
        }
    }

    // Authenticate with the I2PControl JSON-RPC API using the configured password.
    // Stores the obtained token in the AppState's Mutex and returns it.
    async fn authenticate(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Construct the JSON-RPC request body for the 'Authenticate' method
        let req_body = serde_json::json!({
            "id": "1", // Request ID (can be anything)
            "method": "Authenticate",
            "params": { "API": 1, "Password": self.password },
            "jsonrpc": "2.0"
        });

        // Send the POST request to the API endpoint
        let response = self.api_client.post(&self.api_url).json(&req_body).send().await?;

        // Check for HTTP errors
        if !response.status().is_success() {
            return Err(format!(
                "Authentication HTTP request failed with status {}",
                response.status()
            )
            .into());
        }

        // Parse the JSON-RPC response
        let rpc: RpcResponse<AuthResult> = response.json().await?;

        // Check for JSON-RPC level errors
        if let Some(err) = rpc.error {
            return Err(format!(
                "Authentication error {}: {}",
                err.code, err.message
            )
            .into());
        }

        // Extract the token from the successful result
        if let Some(result) = rpc.result {
            if let Some(token) = result.token {
                // Store the obtained token within the AppState's Mutex
                {
                    let mut guard = self.token.lock(); // Lock the mutex
                    *guard = Some(token.clone()); // Update the token value
                } // Mutex guard is dropped here, releasing the lock
                info!("Obtained authentication token from I2PControl");
                return Ok(token); // Return the obtained token
            }
        }

        // If no token was found in a successful response, return an error
        Err("Authentication failed: no token received".into())
    }

    // Fetch metrics using the 'RouterInfo' API method and format them for Prometheus.
    // Handles token acquisition and re-authentication if the token expires.
    async fn fetch_metrics(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut did_retry = false; // Flag to prevent infinite retry loops

        loop { // Loop to handle potential re-authentication
            // Get the current token from the mutex
            let current_token = {
                let guard = self.token.lock(); // Lock the mutex
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
                "i2p.router.status", // Request router status string (e.g., "OK", "Testing")
                "i2p.router.version", // Request router version string
                "i2p.router.uptime", // Request uptime in milliseconds
                "i2p.router.net.bw.inbound.1s", // Request inbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.inbound.15s", // Request inbound bandwidth (15s avg, Bps)
                "i2p.router.net.bw.outbound.1s", // Request outbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.outbound.15s", // Request outbound bandwidth (15s avg, Bps)
                "i2p.router.net.status", // Request network status code (numeric)
                "i2p.router.net.tunnels.participating", // Request participating tunnel count (0 or 1 likely)
                "i2p.router.netdb.activepeers", // Request active peer count (floodfills)
                "i2p.router.netdb.knownpeers", // Request known peer count (total RouterInfos)
            ] {
                // Value::Null indicates we want the value for this key
                params.insert((*key).to_string(), Value::Null);
            }
            // Include the authentication token in the parameters
            params.insert("Token".to_string(), Value::String(token.clone()));

            // Construct the full JSON-RPC request body
            let req_body = serde_json::json!({
                "id": "1", // Request ID
                "method": "RouterInfo", // API method name
                "params": params, // Parameters map constructed above
                "jsonrpc": "2.0"
            });

            // Send the POST request to the API endpoint
            let response = self.api_client.post(&self.api_url).json(&req_body).send().await?;

            // Check for HTTP errors
            if !response.status().is_success() {
                return Err(format!(
                    "Metrics HTTP request failed with status {}",
                    response.status()
                )
                .into());
            }

            let rpc: RpcResponse<RouterInfoResult> = response.json().await?;
            // Check for JSON-RPC level errors, handle token expiry with one retry
            if let Some(err) = rpc.error {
                let code = err.code;
                // Known token-expiration codes: -32002, -32003, -32004
                if (code == -32002 || code == -32003 || code == -32004) && !did_retry {
                    warn!("Token error (code {}), re-authenticating...", code);
                    // Clear the potentially invalid token and re-authenticate
                    {
                        let mut guard = self.token.lock();
                        *guard = None;
                    }
                    let _ = self.authenticate().await?;
                    did_retry = true; // only retry once
                    continue;
                }
                return Err(format!("RouterInfo error {}: {}", code, err.message).into());
            }
            let data = rpc.result.ok_or("No RouterInfo result")?;

            // Build the Prometheus output
            let mut output = String::with_capacity(1024);

            // Router status
            if let Some(status) = &data.router_status {
                output += "# HELP i2p_router_status Router status string\n";
                output += "# TYPE i2p_router_status gauge\n";
                let status_value = status.parse::<f64>().unwrap_or(1.0);
                output += &format!("i2p_router_status {}\n", status_value);
            }

            // Router version
            if let Some(version) = &data.router_version {
                output += "# HELP i2p_router_version_info Router version information\n";
                output += "# TYPE i2p_router_version_info gauge\n";
                output += &format!("i2p_router_version_info{{version=\"{}\"}} 1\n", version);
            }

            // Metric: Router Uptime (convert ms to seconds)
            if let Some(val) = data.router_uptime {
                // Handle potential number or string representation from API
                let seconds = match val {
                    Value::Number(num) => num.as_f64().unwrap_or(0.0) / 1000.0, // Convert ms to s
                    Value::String(s) => s.parse::<f64>().unwrap_or(0.0) / 1000.0, // Convert ms to s
                    _ => 0.0, // Default to 0 if type is unexpected
                };
                output += "# HELP i2p_router_uptime_seconds Router uptime in seconds\n";
                output += "# TYPE i2p_router_uptime_seconds gauge\n";
                output += &format!("i2p_router_uptime_seconds {:.3}\n", seconds); // Format to 3 decimal places
            }

            // Metrics: Bandwidth (Inbound/Outbound, 1s/15s intervals, Bytes/sec)
            if data.bw_inbound_1s.is_some() || data.bw_inbound_15s.is_some() {
                output += "# HELP i2p_router_bandwidth_inbound_bytes_per_second Inbound bandwidth in bytes/sec\n";
                output += "# TYPE i2p_router_bandwidth_inbound_bytes_per_second gauge\n";
                if let Some(bw) = data.bw_inbound_1s { // 1-second average
                    output += &format!("i2p_router_bandwidth_inbound_bytes_per_second{{interval=\"1s\"}} {}\n", bw);
                }
                if let Some(bw) = data.bw_inbound_15s { // 15-second average
                    output += &format!("i2p_router_bandwidth_inbound_bytes_per_second{{interval=\"15s\"}} {}\n", bw);
                }
            }
            if data.bw_outbound_1s.is_some() || data.bw_outbound_15s.is_some() {
                output += "# HELP i2p_router_bandwidth_outbound_bytes_per_second Outbound bandwidth in bytes/sec\n";
                output += "# TYPE i2p_router_bandwidth_outbound_bytes_per_second gauge\n";
                if let Some(bw) = data.bw_outbound_1s { // 1-second average
                    output += &format!("i2p_router_bandwidth_outbound_bytes_per_second{{interval=\"1s\"}} {}\n", bw);
                }
                if let Some(bw) = data.bw_outbound_15s { // 15-second average
                    output += &format!("i2p_router_bandwidth_outbound_bytes_per_second{{interval=\"15s\"}} {}\n", bw);
                }
            }

            // Metric: Network Status Code
            if let Some(status) = data.net_status {
                output += "# HELP i2p_router_network_status_code Network status code (numeric)\n";
                output += "# TYPE i2p_router_network_status_code gauge\n";
                output += &format!("i2p_router_network_status_code {}\n", status);
            }

            // Metric: Participating Tunnels
            if let Some(count) = data.tunnels_participating {
                output += "# HELP i2p_router_tunnels_participating Number of active participating transit tunnels\n";
                output += "# TYPE i2p_router_tunnels_participating gauge\n";
                output += &format!("i2p_router_tunnels_participating {}\n", count);
            }

            // Metrics: NetDB Peer Statistics
            if let Some(count) = data.netdb_activepeers {
                output += "# HELP i2p_router_netdb_activepeers Number of active known peers in NetDB\n";
                output += "# TYPE i2p_router_netdb_activepeers gauge\n";
                output += &format!("i2p_router_netdb_activepeers {}\n", count);
            }
            if let Some(count) = data.netdb_knownpeers {
                output += "# HELP i2p_router_netdb_knownpeers Total number of known peers (RouterInfos) in NetDB\n";
                output += "# TYPE i2p_router_netdb_knownpeers gauge\n";
                output += &format!("i2p_router_netdb_knownpeers {}\n", count);
            }

            // Metric: Exporter Version (info gauge)
            output += "# HELP i2pd_exporter_version_info Version of the i2pd-exporter\n";
            output += "# TYPE i2pd_exporter_version_info gauge\n";
            // Use CARGO_PKG_VERSION env var set at compile time
            output += &format!(
                "i2pd_exporter_version_info{{version=\"{}\"}} 1\n",
                env!("CARGO_PKG_VERSION")
            );

            return Ok(output);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments (handles --version automatically)
    let _cli = Cli::parse();

    env_logger::init();

    // Configuration
    let i2p_addr = std::env::var("I2PCONTROL_ADDRESS")
        .unwrap_or_else(|_| "https://127.0.0.1:7650".to_string());
    let i2p_password = std::env::var("I2PCONTROL_PASSWORD")
        .unwrap_or_else(|_| "itoopie".to_string());
    let listen_addr = std::env::var("METRICS_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9600".to_string());
    let http_timeout = std::env::var("HTTP_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "60".to_string())
        .parse::<u64>()
        .unwrap_or(60);
    let listen_addr: SocketAddr = listen_addr.parse().expect("Invalid listen address");

    info!("Starting I2PControl exporter on {} (target: {})", listen_addr, i2p_addr);

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
        match st.fetch_metrics().await {
            Ok(metrics) => {
                let reply = warp::reply::with_status(metrics, warp::http::StatusCode::OK);
                let reply = warp::reply::with_header(
                    reply,
                    "Content-Type",
                    "text/plain; version=0.0.4"
                );
                Ok(reply)
            }
            Err(err) => {
                error!("Failed to fetch metrics: {}", err);
                let error_body = "Error retrieving metrics".to_string();
                let reply = warp::reply::with_status(error_body, warp::http::StatusCode::INTERNAL_SERVER_ERROR);
                let reply = warp::reply::with_header(reply, "Content-Type", "text/plain; version=0.0.4");
                Ok(reply)
            }
        }
    }

    // Warp filter for GET /metrics
    let route_metrics = warp::path("metrics")
        .and(warp::any().map(move || state.clone()))
        .and_then(metrics_handler);

    // Fallback 404 for anything else
    let route_404 = warp::any().map(|| {
        warp::reply::with_status("Not Found", warp::http::StatusCode::NOT_FOUND)
    });

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
    let (_addr, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(listen_addr, shutdown_signal);

    server.await;

    Ok(())
}
