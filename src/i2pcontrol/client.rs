// I2PControl client implementation

use std::time::{Duration, Instant};

use serde_json::Value;

use super::rpc::rpc_call;
use super::types::RouterInfoResult;

const ROUTER_INFO_KEYS_BATCH_1: &[&str] = &[
    "i2p.router.status",              // Router status as string "1" or "0"
    "i2p.router.version",             // Request router version string
    "i2p.router.uptime",              // Request uptime in milliseconds
    "i2p.router.net.bw.inbound.1s",   // Request inbound bandwidth (1s avg, Bps)
    "i2p.router.net.bw.inbound.15s",  // Request inbound bandwidth (15s avg, Bps)
    "i2p.router.net.bw.outbound.1s",  // Request outbound bandwidth (1s avg, Bps)
    "i2p.router.net.bw.outbound.15s", // Request outbound bandwidth (15s avg, Bps)
    "i2p.router.net.bw.transit.15s",  // Request transit bandwidth (15s avg, Bps)
    "i2p.router.net.status", // Request IPv4 network status code (0 OK, 1 Firewalled, 2 Unknown, 3 Proxy, 4 Mesh, 5 Stan)
    "i2p.router.net.status.v6", // Request IPv6 network status code (optional, same mapping)
    "i2p.router.net.error",  // Request IPv4 network error code
    "i2p.router.net.error.v6", // Request IPv6 network error code
    "i2p.router.net.testing", // Request IPv4 network testing flag
    "i2p.router.net.testing.v6", // Request IPv6 network testing flag
];

const ROUTER_INFO_KEYS_BATCH_2: &[&str] = &[
    "i2p.router.net.tunnels.participating", // Request participating tunnel count (0 or 1 likely)
    "i2p.router.net.tunnels.inbound",       // Request inbound tunnel count
    "i2p.router.net.tunnels.outbound",      // Request outbound tunnel count
    "i2p.router.net.tunnels.successrate",   // Request tunnel success rate (percent integer)
    "i2p.router.net.tunnels.totalsuccessrate", // Request aggregate tunnel success rate (percent integer)
    "i2p.router.net.tunnels.queue",            // Request tunnel build queue size
    "i2p.router.net.tunnels.tbmqueue",         // Request transit build message queue size
    "i2p.router.netdb.activepeers",            // Request active peer count (floodfills)
    "i2p.router.netdb.knownpeers",             // Request known peer count (total RouterInfos)
    "i2p.router.netdb.floodfills",             // Request floodfill routers known to NetDB
    "i2p.router.netdb.leasesets",              // Request LeaseSets known to NetDB
    "i2p.router.net.total.received.bytes",     // Request total received bytes
    "i2p.router.net.total.sent.bytes",         // Request total sent bytes
    "i2p.router.net.total.transit.bytes",      // Request total transit bytes transmitted
];

fn build_router_info_params(keys: &[&str]) -> Value {
    let mut params = serde_json::Map::new();
    for key in keys {
        // Use empty string instead of null; some i2pd builds reject nulls with parse errors.
        params.insert((*key).to_string(), Value::String(String::new()));
    }
    Value::Object(params)
}

// Holds shared state for the application, including the API client,
// and scrape configuration.
pub struct I2pControlClient {
    pub api_client: reqwest::Client, // HTTP client for making API requests
    pub api_url: String,             // Full URL for the I2PControl JSON-RPC endpoint
    pub max_scrape_timeout: Duration, // Hard cap for header-derived scrape timeout
}

impl I2pControlClient {
    // Creates a new AppState instance.
    pub fn new(api_client: reqwest::Client, api_url: String, max_scrape_timeout: Duration) -> Self {
        I2pControlClient {
            api_client,
            api_url,
            max_scrape_timeout,
        }
    }

    // Fetch router information from the I2PControl API.
    pub async fn fetch_router_info(
        &self,
        overall_timeout: Duration,
    ) -> Result<RouterInfoResult, Box<dyn std::error::Error + Send + Sync>> {
        let deadline = Instant::now() + overall_timeout;
        let mut combined = RouterInfoResult::default();

        for (batch_idx, keys) in [ROUTER_INFO_KEYS_BATCH_1, ROUTER_INFO_KEYS_BATCH_2]
            .iter()
            .enumerate()
        {
            let now = Instant::now();
            let rem = if now >= deadline {
                Duration::from_millis(0)
            } else {
                deadline.saturating_duration_since(now)
            };
            if rem.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "deadline exceeded before RouterInfo batch {}",
                        batch_idx + 1
                    ),
                )
                .into());
            }
            let params = build_router_info_params(keys);

            let data = rpc_call::<RouterInfoResult>(
                &self.api_client,
                &self.api_url,
                "RouterInfo",
                params,
                rem,
            )
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })?;

            combined.merge_from(data);
        }

        Ok(combined)
    }
}
