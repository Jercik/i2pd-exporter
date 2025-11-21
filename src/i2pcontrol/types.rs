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
    #[serde(rename = "i2p.router.net.bw.transit.15s")]
    pub bw_transit_15s: Option<f64>,
    #[serde(rename = "i2p.router.net.status")]
    pub net_status: Option<u8>,
    #[serde(rename = "i2p.router.net.status.v6")]
    pub net_status_v6: Option<u8>,
    #[serde(rename = "i2p.router.net.error")]
    pub net_error: Option<u8>,
    #[serde(rename = "i2p.router.net.error.v6")]
    pub net_error_v6: Option<u8>,
    #[serde(rename = "i2p.router.net.testing")]
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub net_testing: Option<u8>,
    #[serde(rename = "i2p.router.net.testing.v6")]
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub net_testing_v6: Option<u8>,
    #[serde(rename = "i2p.router.net.tunnels.participating")]
    pub tunnels_participating: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.inbound")]
    pub tunnels_inbound: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.outbound")]
    pub tunnels_outbound: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.successrate")]
    pub tunnels_successrate: Option<f64>,
    #[serde(rename = "i2p.router.net.tunnels.totalsuccessrate")]
    pub tunnels_total_successrate: Option<f64>,
    #[serde(rename = "i2p.router.net.tunnels.queue")]
    pub tunnels_queue: Option<u64>,
    #[serde(rename = "i2p.router.net.tunnels.tbmqueue")]
    pub tunnels_tbmqueue: Option<u64>,
    #[serde(rename = "i2p.router.netdb.activepeers")]
    pub netdb_activepeers: Option<u64>,
    #[serde(rename = "i2p.router.netdb.knownpeers")]
    pub netdb_knownpeers: Option<u64>,
    #[serde(rename = "i2p.router.netdb.floodfills")]
    pub netdb_floodfills: Option<u64>,
    #[serde(rename = "i2p.router.netdb.leasesets")]
    pub netdb_leasesets: Option<u64>,
    #[serde(rename = "i2p.router.net.total.received.bytes")]
    pub net_total_received_bytes: Option<f64>,
    #[serde(rename = "i2p.router.net.total.sent.bytes")]
    pub net_total_sent_bytes: Option<f64>,
    #[serde(rename = "i2p.router.net.transit.sent.bytes")]
    pub net_transit_sent_bytes: Option<f64>,
}

impl RouterInfoResult {
    // Merge data from another RouterInfoResult, preferring values from `other` when present.
    pub fn merge_from(&mut self, other: RouterInfoResult) {
        if let Some(v) = other.router_status {
            self.router_status = Some(v);
        }
        if let Some(v) = other.router_version {
            self.router_version = Some(v);
        }
        if let Some(v) = other.router_uptime {
            self.router_uptime = Some(v);
        }
        if let Some(v) = other.bw_inbound_1s {
            self.bw_inbound_1s = Some(v);
        }
        if let Some(v) = other.bw_inbound_15s {
            self.bw_inbound_15s = Some(v);
        }
        if let Some(v) = other.bw_outbound_1s {
            self.bw_outbound_1s = Some(v);
        }
        if let Some(v) = other.bw_outbound_15s {
            self.bw_outbound_15s = Some(v);
        }
        if let Some(v) = other.bw_transit_15s {
            self.bw_transit_15s = Some(v);
        }
        if let Some(v) = other.net_status {
            self.net_status = Some(v);
        }
        if let Some(v) = other.net_status_v6 {
            self.net_status_v6 = Some(v);
        }
        if let Some(v) = other.net_error {
            self.net_error = Some(v);
        }
        if let Some(v) = other.net_error_v6 {
            self.net_error_v6 = Some(v);
        }
        if let Some(v) = other.net_testing {
            self.net_testing = Some(v);
        }
        if let Some(v) = other.net_testing_v6 {
            self.net_testing_v6 = Some(v);
        }
        if let Some(v) = other.tunnels_participating {
            self.tunnels_participating = Some(v);
        }
        if let Some(v) = other.tunnels_inbound {
            self.tunnels_inbound = Some(v);
        }
        if let Some(v) = other.tunnels_outbound {
            self.tunnels_outbound = Some(v);
        }
        if let Some(v) = other.tunnels_successrate {
            self.tunnels_successrate = Some(v);
        }
        if let Some(v) = other.tunnels_total_successrate {
            self.tunnels_total_successrate = Some(v);
        }
        if let Some(v) = other.tunnels_queue {
            self.tunnels_queue = Some(v);
        }
        if let Some(v) = other.tunnels_tbmqueue {
            self.tunnels_tbmqueue = Some(v);
        }
        if let Some(v) = other.netdb_activepeers {
            self.netdb_activepeers = Some(v);
        }
        if let Some(v) = other.netdb_knownpeers {
            self.netdb_knownpeers = Some(v);
        }
        if let Some(v) = other.netdb_floodfills {
            self.netdb_floodfills = Some(v);
        }
        if let Some(v) = other.netdb_leasesets {
            self.netdb_leasesets = Some(v);
        }
        if let Some(v) = other.net_total_received_bytes {
            self.net_total_received_bytes = Some(v);
        }
        if let Some(v) = other.net_total_sent_bytes {
            self.net_total_sent_bytes = Some(v);
        }
        if let Some(v) = other.net_transit_sent_bytes {
            self.net_transit_sent_bytes = Some(v);
        }
    }
}
