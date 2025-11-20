// I2PControl API type definitions

use serde::Deserialize;
use serde_aux::prelude::*;
// (Was: use serde_repr::Deserialize_repr;)

// Result structure for the 'Authenticate' method
#[derive(Debug, Deserialize, Default)]
pub struct AuthResult {
    #[serde(rename = "Token")]
    pub token: Option<String>,
}

// Result structure for the 'RouterInfo' method, containing various metrics
#[derive(Debug, Deserialize, Default)]
pub struct RouterInfoResult {
    #[serde(rename = "i2p.router.status")]
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub router_status: Option<u8>,
    #[serde(rename = "i2p.router.version")]
    pub router_version: Option<String>,
    #[serde(rename = "i2p.router.uptime")]
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub router_uptime: Option<u64>,
    #[serde(rename = "i2p.router.net.bw.inbound.1s")]
    pub bw_inbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.inbound.15s")]
    pub bw_inbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.1s")]
    pub bw_outbound_1s: Option<f64>,
    #[serde(rename = "i2p.router.net.bw.outbound.15s")]
    pub bw_outbound_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.status")]
    pub net_status: Option<u8>,
    #[serde(rename = "i2p.router.net.status.v6")]
    pub net_status_v6: Option<u8>,
    #[serde(rename = "i2p.router.net.tunnels.participating")]
    pub tunnels_participating: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.successrate")]
    pub tunnels_successrate: Option<f64>,
    #[serde(rename = "i2p.router.netdb.activepeers")]
    pub netdb_activepeers: Option<u64>,
    #[serde(rename = "i2p.router.netdb.knownpeers")]
    pub netdb_knownpeers: Option<u64>,
    #[serde(rename = "i2p.router.net.total.received.bytes")]
    pub net_total_received_bytes: Option<f64>,
    #[serde(rename = "i2p.router.net.total.sent.bytes")]
    pub net_total_sent_bytes: Option<f64>,
}

// (Old block removed)
