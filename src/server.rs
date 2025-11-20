// HTTP server handlers

use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{error, warn};
use warp::http::HeaderMap;
use warp::{self, Filter, Reply};

use crate::i2pcontrol::rpc::RpcCallError;
use crate::i2pcontrol::I2pControlClient;
use crate::metrics::encode_metrics_text;
use crate::version;

// Compute effective timeout strictly from the Prometheus header.
// Returns None if the header is missing or invalid. Applies a 0.5s margin only when header > 3s,
// and clamps the final value to at least 0.1s.
fn effective_timeout(headers: &HeaderMap, hard_max: Duration) -> Option<Duration> {
    const MARGIN: f64 = 0.5;
    const MARGIN_THRESHOLD: f64 = 3.0; // apply margin only when header > 3s

    let secs = headers
        .get("X-Prometheus-Scrape-Timeout-Seconds")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())?;

    let adjusted = if secs > MARGIN_THRESHOLD {
        secs - MARGIN
    } else {
        secs
    };
    let adjusted = adjusted.max(0.1);
    let capped = adjusted.min(hard_max.as_secs_f64());
    Some(Duration::from_secs_f64(capped))
}

// Very small Accept negotiation: prefer OpenMetrics when the client
// either accepts it explicitly or does not specify a preference.
// We always emit OpenMetrics text, so the content type must match.
const OM_CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";
fn choose_content_type(headers: &HeaderMap) -> &'static str {
    match headers.get("Accept").and_then(|v| v.to_str().ok()) {
        Some(accept) => {
            let a = accept.to_ascii_lowercase();
            if a.contains("application/openmetrics-text") || a.contains("*/*") {
                OM_CONTENT_TYPE
            } else {
                // We only encode OpenMetrics; be precise about what we return.
                OM_CONTENT_TYPE
            }
        }
        None => OM_CONTENT_TYPE,
    }
}

// Define a small async handler function for /metrics
pub async fn metrics_handler(
    st: Arc<I2pControlClient>,
    headers: HeaderMap,
) -> Result<impl warp::Reply, warp::Rejection> {
    let t0 = Instant::now();

    // Require the Prometheus timeout header and compute the effective timeout
    let Some(effective_timeout) = effective_timeout(&headers, st.max_scrape_timeout) else {
        let msg = "missing or invalid X-Prometheus-Scrape-Timeout-Seconds header".to_string();
        let reply = warp::reply::with_status(msg, warp::http::StatusCode::BAD_REQUEST);
        let reply = warp::reply::with_header(reply, "Content-Type", choose_content_type(&headers));
        let reply = warp::reply::with_header(reply, "Cache-Control", "no-store");
        return Ok(reply);
    };

    // Attempt to fetch target metrics within the overall scrape budget
    let (status_code, router_data, scrape_error) = match tokio::time::timeout(
        effective_timeout,
        st.fetch_router_info(effective_timeout),
    )
    .await
    {
        Err(_elapsed) => {
            // Outer scrape budget elapsed; warn with computed budget for observability
            warn!(
                "Scrape timed out; effective budget {:.3}s",
                effective_timeout.as_secs_f64()
            );
            (warp::http::StatusCode::GATEWAY_TIMEOUT, None, 1u8)
        }
        Ok(Ok(data)) => (warp::http::StatusCode::OK, Some(data), 0u8),
        Ok(Err(err)) => {
            error!("Failed to fetch metrics: {}", err);
            // If the inner error is a timeout (reqwest/io), surface 504; else 500.
            let status = if let Some(rpc) = err.downcast_ref::<RpcCallError>() {
                match rpc {
                    RpcCallError::Transport(e) if e.is_timeout() => {
                        warp::http::StatusCode::GATEWAY_TIMEOUT
                    }
                    _ => warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                }
            } else if let Some(ioe) = err.downcast_ref::<std::io::Error>() {
                if ioe.kind() == std::io::ErrorKind::TimedOut {
                    warp::http::StatusCode::GATEWAY_TIMEOUT
                } else {
                    warp::http::StatusCode::INTERNAL_SERVER_ERROR
                }
            } else {
                warp::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, None, 1u8)
        }
    };

    // Encode all metrics (router + exporter) via prometheus-client once.
    let scrape_seconds = t0.elapsed().as_secs_f64();
    let body = encode_metrics_text(
        router_data.as_ref(),
        scrape_seconds,
        Some(effective_timeout.as_secs_f64()),
        scrape_error,
        version::VERSION,
    );

    let reply = warp::reply::with_status(body, status_code);
    let reply = warp::reply::with_header(reply, "Content-Type", choose_content_type(&headers));
    let reply = warp::reply::with_header(reply, "Cache-Control", "no-store");
    Ok(reply)
}

// Adapter that converts the Reply into a concrete Response
pub async fn metrics_handler_response(
    st: Arc<I2pControlClient>,
    headers: HeaderMap,
) -> Result<warp::reply::Response, warp::Rejection> {
    let r = metrics_handler(st, headers).await?;
    Ok(r.into_response())
}

// Expose a composed routes filter so main can stay lean
pub fn routes(
    state: Arc<I2pControlClient>,
) -> impl Filter<Extract = (warp::reply::Response,), Error = warp::Rejection> + Clone {
    let route_metrics = warp::path("metrics")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::any().map(move || state.clone()))
        .and(warp::header::headers_cloned())
        .and_then(metrics_handler_response);

    let route_404 = warp::path::end().map(|| {
        warp::reply::with_status("Not Found", warp::http::StatusCode::NOT_FOUND).into_response()
    });

    route_metrics.or(route_404).unify()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_no_header_is_none() {
        let headers = HeaderMap::new();
        assert!(effective_timeout(&headers, Duration::from_secs(60)).is_none());
    }

    #[test]
    fn timeout_smaller_header_wins_with_margin_above_threshold() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Prometheus-Scrape-Timeout-Seconds",
            "3.1".parse().unwrap(),
        );
        // 3.1 > 3.0 -> apply margin: 3.1 - 0.5 = 2.6s
        let eff = effective_timeout(&headers, Duration::from_secs(60)).unwrap();
        assert!((eff.as_secs_f64() - 2.6).abs() < 1e-9);
    }

    #[test]
    fn timeout_large_header_is_capped_by_max() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Prometheus-Scrape-Timeout-Seconds",
            "30.0".parse().unwrap(),
        );
        // 30.0 - 0.5 = 29.5s, but cap at 10s
        let eff = effective_timeout(&headers, Duration::from_secs(10)).unwrap();
        assert!((eff.as_secs_f64() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn timeout_small_header_kept_no_margin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Prometheus-Scrape-Timeout-Seconds",
            "0.2".parse().unwrap(),
        );
        // 0.2 <= 3.0 -> no margin; remains 0.2s
        let eff = effective_timeout(&headers, Duration::from_secs(60)).unwrap();
        assert!((eff.as_secs_f64() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn timeout_header_negative_value_clamped_to_min() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Prometheus-Scrape-Timeout-Seconds", "-5".parse().unwrap());
        // -5.0 - 0.5 => clamped to 0.1s, min with default -> 0.1s
        let eff = effective_timeout(&headers, Duration::from_secs(60)).unwrap();
        assert!((eff.as_secs_f64() - 0.1).abs() < 1e-9);
    }

    #[test]
    fn timeout_header_non_numeric_is_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Prometheus-Scrape-Timeout-Seconds",
            "not-a-number".parse().unwrap(),
        );
        assert!(effective_timeout(&headers, Duration::from_secs(60)).is_none());
    }
    // No default cap test anymore
}
