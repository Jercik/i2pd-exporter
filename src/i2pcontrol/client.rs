// I2PControl client implementation

use std::time::{Duration, Instant};

use log::{info, warn};
use serde_json::Value;
use tokio::sync::Mutex;

use super::rpc::{rpc_call, RpcCallError};
use super::types::{AuthResult, RouterInfoResult};

// Holds shared state for the application, including the API client,
// configuration, and the authentication token (protected by a Mutex).
pub struct I2pControlClient {
    pub api_client: reqwest::Client, // HTTP client for making API requests
    pub api_url: String,             // Full URL for the I2PControl JSON-RPC endpoint
    pub password: String,            // Password for the I2PControl API
    pub token: Mutex<Option<String>>, // Current authentication token (None if not authenticated)
    // Singleflight-style mutex to ensure only one in-flight authentication
    // happens at a time across concurrent scrapes.
    auth_lock: Mutex<()>,
    pub max_scrape_timeout: Duration, // Hard cap for header-derived scrape timeout
}

impl I2pControlClient {
    // Creates a new AppState instance.
    pub fn new(
        api_client: reqwest::Client,
        api_url: String,
        password: String,
        max_scrape_timeout: Duration,
    ) -> Self {
        I2pControlClient {
            api_client,
            api_url,
            password,
            token: Mutex::new(None),
            auth_lock: Mutex::new(()),
            max_scrape_timeout,
        }
    }

    // Authenticate with the I2PControl JSON-RPC API using the configured password.
    // Stores the obtained token in the AppState's Mutex and returns it.
    pub async fn authenticate(
        &self,
        timeout: Duration,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Ensure only one concurrent authentication attempt is in-flight.
        let _flight = self.auth_lock.lock().await;

        // Double-check if another task already refreshed the token while we waited.
        if let Some(existing) = { self.token.lock().await.clone() } {
            return Ok(existing);
        }

        let params = serde_json::json!({ "API": 1, "Password": self.password });
        let result: AuthResult = rpc_call(
            &self.api_client,
            &self.api_url,
            "Authenticate",
            params,
            timeout,
        )
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

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
    pub async fn fetch_router_info(
        &self,
        overall_timeout: Duration,
    ) -> Result<RouterInfoResult, Box<dyn std::error::Error + Send + Sync>> {
        let deadline = Instant::now() + overall_timeout;
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
                    let now = Instant::now();
                    let rem = if now >= deadline {
                        Duration::from_millis(0)
                    } else {
                        deadline.saturating_duration_since(now)
                    };
                    if rem.is_zero() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "deadline exceeded before authentication",
                        )
                        .into());
                    }
                    self.authenticate(rem).await?
                }
            };

            // Build the parameters for the 'RouterInfo' JSON-RPC request.
            // We request specific keys related to router status, bandwidth, network, etc.
            let mut params = serde_json::Map::new();
            for key in &[
                "i2p.router.status",                       // Router status as string "1" or "0"
                "i2p.router.version",                      // Request router version string
                "i2p.router.uptime",                       // Request uptime in milliseconds
                "i2p.router.net.bw.inbound.1s", // Request inbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.inbound.15s", // Request inbound bandwidth (15s avg, Bps)
                "i2p.router.net.bw.outbound.1s", // Request outbound bandwidth (1s avg, Bps)
                "i2p.router.net.bw.outbound.15s", // Request outbound bandwidth (15s avg, Bps)
                "i2p.router.net.bw.transit.15s", // Request transit bandwidth (15s avg, Bps)
                "i2p.router.net.status", // Request IPv4 network status code (0 OK, 1 Firewalled, 2 Unknown, 3 Proxy, 4 Mesh, 5 Stan)
                "i2p.router.net.status.v6", // Request IPv6 network status code (optional, same mapping)
                "i2p.router.net.error",     // Request IPv4 network error code
                "i2p.router.net.error.v6",  // Request IPv6 network error code
                "i2p.router.net.testing",   // Request IPv4 network testing flag
                "i2p.router.net.testing.v6", // Request IPv6 network testing flag
                "i2p.router.net.tunnels.participating", // Request participating tunnel count (0 or 1 likely)
                "i2p.router.net.tunnels.inbound",       // Request inbound tunnel count
                "i2p.router.net.tunnels.outbound",      // Request outbound tunnel count
                "i2p.router.net.tunnels.successrate", // Request tunnel success rate (percent integer)
                "i2p.router.net.tunnels.totalsuccessrate", // Request aggregate tunnel success rate (percent integer)
                "i2p.router.net.tunnels.queue",            // Request tunnel build queue size
                "i2p.router.net.tunnels.tbmqueue", // Request transit build message queue size
                "i2p.router.netdb.activepeers",    // Request active peer count (floodfills)
                "i2p.router.netdb.knownpeers",     // Request known peer count (total RouterInfos)
                "i2p.router.netdb.floodfills",     // Request floodfill routers known to NetDB
                "i2p.router.netdb.leasesets",      // Request LeaseSets known to NetDB
                "i2p.router.net.total.received.bytes", // Request total received bytes
                "i2p.router.net.total.sent.bytes", // Request total sent bytes
                "i2p.router.net.transit.sent.bytes", // Request total transit-sent bytes
            ] {
                // Use empty string instead of null; some i2pd builds reject nulls with parse errors.
                params.insert((*key).to_string(), Value::String(String::new()));
            }
            // Include the authentication token in the parameters
            params.insert("Token".to_string(), Value::String(token.clone()));

            // Perform JSON-RPC call, handle token expiry with one retry
            let now = Instant::now();
            let rem = if now >= deadline {
                Duration::from_millis(0)
            } else {
                deadline.saturating_duration_since(now)
            };
            if rem.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "deadline exceeded before RouterInfo",
                )
                .into());
            }
            let data = match rpc_call::<RouterInfoResult>(
                &self.api_client,
                &self.api_url,
                "RouterInfo",
                Value::Object(params),
                rem,
            )
            .await
            {
                Ok(data) => data,
                Err(err) => {
                    let is_token_err = matches!(
                        err,
                        RpcCallError::Rpc {
                            code: -32004..=-32002,
                            ..
                        }
                    );
                    if is_token_err && !did_retry {
                        warn!("Token error, re-authenticating...: {}", err);
                        {
                            let mut guard = self.token.lock().await;
                            *guard = None;
                        }
                        let now = Instant::now();
                        let rem = if now >= deadline {
                            Duration::from_millis(0)
                        } else {
                            deadline.saturating_duration_since(now)
                        };
                        if rem.is_zero() {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "deadline exceeded before re-authentication",
                            )
                            .into());
                        }
                        let _ = self.authenticate(rem).await?;
                        did_retry = true;
                        continue;
                    }
                    return Err(Box::new(err));
                }
            };

            return Ok(data);
        }
    }
}
