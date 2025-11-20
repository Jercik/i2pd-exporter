use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::i2pcontrol::types::RouterInfoResult;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct DirectionWindowLabels {
    direction: &'static str,
    window: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct DirectionLabels {
    direction: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StateLabel {
    state: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ExporterBuildInfoLabels {
    version: &'static str,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct RouterBuildInfoLabels {
    // String labels are supported by the derive; we keep the router version as-is.
    version: String,
}

fn bucket_state(code: u8, label: &str) -> f64 {
    // Set exactly one state to 1.0 for known codes 0..=4.
    // For any unknown code, map to the "unknown" bucket only.
    static UNKNOWN_NET_STATUS_LOGGED: AtomicBool = AtomicBool::new(false);
    match code {
        0 => (label == "ok") as u8 as f64,
        1 => (label == "firewalled") as u8 as f64,
        2 => (label == "unknown") as u8 as f64,
        3 => (label == "proxy") as u8 as f64,
        4 => (label == "mesh") as u8 as f64,
        _ => {
            if !UNKNOWN_NET_STATUS_LOGGED.swap(true, Ordering::Relaxed) {
                log::warn!("Observed unknown net status code: {}", code);
            }
            (label == "unknown") as u8 as f64
        }
    }
}

/// Render Prometheus text for the given router data and exporter self-metrics.
/// - `data`: router metrics (None when fetch failed or timed out)
/// - `scrape_duration_seconds`: wall time of the entire scrape handler
/// - `effective_timeout_seconds`: optional computed budget (if available from header handling)
/// - `last_scrape_error`: 0 on success, 1 on error
/// - `exporter_version`: exporter build version label
pub fn encode_metrics_text(
    data: Option<&RouterInfoResult>,
    scrape_duration_seconds: f64,
    effective_timeout_seconds: Option<f64>,
    last_scrape_error: u8,
    exporter_version: &'static str,
) -> String {
    let mut registry = Registry::default();

    if let Some(d) = data {
        add_router_metrics(&mut registry, d);
    }

    add_exporter_metrics(
        &mut registry,
        exporter_version,
        scrape_duration_seconds,
        effective_timeout_seconds,
        last_scrape_error,
    );

    let mut buf = String::new();
    // Ignore encode errors into buf; String implements fmt::Write.
    let _ = encode(&mut buf, &registry);
    buf
}

fn add_router_metrics(registry: &mut Registry, d: &RouterInfoResult) {
    // i2p_router_status
    if let Some(status) = d.router_status {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register("i2p_router_status", "Router status (1 or 0)", g.clone());
        g.set(status as f64);
    }

    // i2p_router_build_info{version}
    if let Some(version) = &d.router_version {
        let fam = Family::<RouterBuildInfoLabels, Gauge<f64, AtomicU64>>::default();
        registry.register(
            "i2p_router_build_info",
            "Router build information",
            fam.clone(),
        );
        fam.get_or_create(&RouterBuildInfoLabels {
            version: version.clone(),
        })
        .set(1.0);
    }

    // i2p_router_uptime_seconds
    if let Some(ms) = d.router_uptime {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_uptime_seconds",
            "Router uptime in seconds",
            g.clone(),
        );
        g.set((ms as f64) / 1000.0);
    }

    // i2p_router_net_bw_bytes_per_second{direction,window}
    let any_bw = d.bw_inbound_1s.is_some()
        || d.bw_inbound_15s.is_some()
        || d.bw_outbound_1s.is_some()
        || d.bw_outbound_15s.is_some();
    if any_bw {
        let fam = Family::<DirectionWindowLabels, Gauge<f64, AtomicU64>>::default();
        registry.register(
            "i2p_router_net_bw_bytes_per_second",
            "Router bandwidth in bytes/sec",
            fam.clone(),
        );

        if let Some(v) = d.bw_inbound_1s {
            fam.get_or_create(&DirectionWindowLabels {
                direction: "inbound",
                window: "1s",
            })
            .set(v);
        }
        if let Some(v) = d.bw_inbound_15s {
            fam.get_or_create(&DirectionWindowLabels {
                direction: "inbound",
                window: "15s",
            })
            .set(v);
        }
        if let Some(v) = d.bw_outbound_1s {
            fam.get_or_create(&DirectionWindowLabels {
                direction: "outbound",
                window: "1s",
            })
            .set(v);
        }
        if let Some(v) = d.bw_outbound_15s {
            fam.get_or_create(&DirectionWindowLabels {
                direction: "outbound",
                window: "15s",
            })
            .set(v);
        }
    }

    // i2p_router_net_status{state} + i2p_router_net_status_code (IPv4)
    if let Some(code) = d.net_status {
        let fam = Family::<StateLabel, Gauge<f64, AtomicU64>>::default();
        registry.register(
            "i2p_router_net_status",
            "IPv4 network status as states (ok, firewalled, unknown, proxy, mesh)",
            fam.clone(),
        );
        for label in ["ok", "firewalled", "unknown", "proxy", "mesh"] {
            fam.get_or_create(&StateLabel { state: label })
                .set(bucket_state(code, label));
        }

        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_net_status_code",
            "IPv4 network status code (0=OK, 1=Firewalled, 2=Unknown, 3=Proxy, 4=Mesh)",
            g.clone(),
        );
        g.set(code as f64);
    }

    // i2p_router_net_status_v6{state} + i2p_router_net_status_v6_code (IPv6)
    if let Some(code) = d.net_status_v6 {
        let fam = Family::<StateLabel, Gauge<f64, AtomicU64>>::default();
        registry.register(
            "i2p_router_net_status_v6",
            "IPv6 network status as states (ok, firewalled, unknown, proxy, mesh)",
            fam.clone(),
        );
        for label in ["ok", "firewalled", "unknown", "proxy", "mesh"] {
            fam.get_or_create(&StateLabel { state: label })
                .set(bucket_state(code, label));
        }

        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_net_status_v6_code",
            "IPv6 network status code (0=OK, 1=Firewalled, 2=Unknown, 3=Proxy, 4=Mesh)",
            g.clone(),
        );
        g.set(code as f64);
    }

    // i2p_router_netdb_activepeers / knownpeers
    if let Some(v) = d.netdb_activepeers {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_netdb_activepeers",
            "Number of active known peers in NetDB",
            g.clone(),
        );
        g.set(v as f64);
    }
    if let Some(v) = d.netdb_knownpeers {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_netdb_knownpeers",
            "Total number of known peers (RouterInfos) in NetDB",
            g.clone(),
        );
        g.set(v as f64);
    }

    // i2p_router_tunnels_participating / _success_ratio
    if let Some(v) = d.tunnels_participating {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_tunnels_participating",
            "Number of active participating transit tunnels",
            g.clone(),
        );
        g.set(v as f64);
    }
    if let Some(percent) = d.tunnels_successrate {
        let ratio = (percent / 100.0).clamp(0.0, 1.0);
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2p_router_tunnels_success_ratio",
            "Tunnel build success rate as a ratio (0..1)",
            g.clone(),
        );
        g.set(ratio);
    }

    // i2p_router_net_bytes_total{direction} (counter)
    let any_totals = d.net_total_received_bytes.is_some() || d.net_total_sent_bytes.is_some();
    if any_totals {
        let fam = Family::<DirectionLabels, Counter<f64>>::default();
        // prometheus_client appends `_total` for counters; register without the suffix
        registry.register(
            "i2p_router_net_bytes",
            "Total network bytes since router start",
            fam.clone(),
        );
        if let Some(v) = d.net_total_received_bytes {
            fam.get_or_create(&DirectionLabels {
                direction: "inbound",
            })
            .inc_by(v);
        }
        if let Some(v) = d.net_total_sent_bytes {
            fam.get_or_create(&DirectionLabels {
                direction: "outbound",
            })
            .inc_by(v);
        }
    }
}

fn add_exporter_metrics(
    registry: &mut Registry,
    exporter_version: &'static str,
    scrape_duration_seconds: f64,
    effective_timeout_seconds: Option<f64>,
    last_scrape_error: u8,
) {
    // i2pd_exporter_build_info{version}
    let fam = Family::<ExporterBuildInfoLabels, Gauge<f64, AtomicU64>>::default();
    registry.register(
        "i2pd_exporter_build_info",
        "Exporter build information",
        fam.clone(),
    );
    fam.get_or_create(&ExporterBuildInfoLabels {
        version: exporter_version,
    })
    .set(1.0);

    // i2pd_exporter_scrape_duration_seconds
    let g = Gauge::<f64, AtomicU64>::default();
    registry.register(
        "i2pd_exporter_scrape_duration_seconds",
        "Duration of last scrape",
        g.clone(),
    );
    g.set(scrape_duration_seconds);

    // i2pd_exporter_effective_scrape_timeout_seconds (optional)
    if let Some(v) = effective_timeout_seconds {
        let g = Gauge::<f64, AtomicU64>::default();
        registry.register(
            "i2pd_exporter_effective_scrape_timeout_seconds",
            "Computed effective scrape timeout budget",
            g.clone(),
        );
        g.set(v);
    }

    // i2pd_exporter_last_scrape_error
    let g = Gauge::<f64, AtomicU64>::default();
    registry.register(
        "i2pd_exporter_last_scrape_error",
        "1 if the last scrape had an error, 0 otherwise",
        g.clone(),
    );
    g.set(last_scrape_error as f64);
}
