# i2pd‑exporter

Tiny, pure‑Rust Prometheus exporter for the **i2pd (C++)** router via **I2PControl JSON‑RPC (API=1)**.
**Not** compatible with the Java I2P router.

- Serves metrics at **`/metrics`** (default listen: `0.0.0.0:9600`)
- Handles token auth and retries once on token errors
- Exposes router metrics (status, uptime, bandwidth, IPv4/IPv6 network status, tunnels, NetDB, totals) + exporter self‑metrics
- Small static binary (LTO, stripped)

---

## Quick start

```bash
# Build
cargo build --release

# Version
./target/release/i2pd-exporter --version

# Run (defaults to https://127.0.0.1:7650 for I2PControl)
RUST_LOG=info ./target/release/i2pd-exporter
```

Optional cross‑build for Linux (x86_64): `./build-linux-docker.sh` → `./dist/i2pd-exporter`.

---

## Configuration

> Provide the **base I2PControl URL without `/jsonrpc`**. The exporter appends `/jsonrpc`.

| CLI flag                       | Env var                      | Default                  | Description                                   |
| ------------------------------ | ---------------------------- | ------------------------ | --------------------------------------------- |
| `--i2pcontrol-address`         | `I2PCONTROL_ADDRESS`         | `https://127.0.0.1:7650` | I2PControl base URL (http or https).          |
| `--i2pcontrol-password`        | `I2PCONTROL_PASSWORD`        | `itoopie`                | I2PControl password.                          |
| `--metrics-listen-addr`        | `METRICS_LISTEN_ADDR`        | `0.0.0.0:9600`           | Address:port for the HTTP server.             |
| `--i2pcontrol-tls-insecure`    | `I2PCONTROL_TLS_INSECURE`    | `false`                  | Accept invalid TLS certs (not recommended).   |
| `--max-scrape-timeout-seconds` | `MAX_SCRAPE_TIMEOUT_SECONDS` | `120`                    | **Hard cap** for the effective scrape budget. |

**TLS tip:** Self‑signed loopback (`127.0.0.1`/`localhost`) is automatically allowed; for remote HTTPS targets, prefer proper certificates.

---

## HTTP

- **GET** `/:` → `404 Not Found`
- **GET** `/metrics` → **OpenMetrics** text format

  - `Content-Type: application/openmetrics-text; version=1.0.0; charset=utf-8`
  - `Cache-Control: no-store`

> Note: The server always emits OpenMetrics text (1.0.0). Prometheus and many agents request this via `Accept: application/openmetrics-text;version=1.0.0`. Some browsers may download the response rather than rendering it inline if OpenMetrics is not explicitly accepted.

---

## Scrape timeout (required header)

Prometheus must send `X-Prometheus-Scrape-Timeout-Seconds`. The exporter computes:

```
candidate = (header_secs > 3.0) ? header_secs - 0.5 : header_secs
effective = min(candidate, MAX_SCRAPE_TIMEOUT_SECONDS)
effective is clamped to >= 0.1s
```

- Missing/invalid header → **400 Bad Request**
- Budget exceeded → **504 Gateway Timeout**
- Self‑metrics always include the computed budget.

---

## Metrics (overview)

**Router:**

- `i2p_router_status`
- `i2p_router_build_info{version}`
- `i2p_router_uptime_seconds`
- `i2p_router_net_bw_bytes_per_second{direction,window}` (`inbound`,`outbound`,`transit`; `1s`,`15s`)
- `i2p_router_net_status{state}` + `i2p_router_net_status_code` (IPv4+IPv6, includes `stan`)
- `i2p_router_net_error{error}` + `i2p_router_net_error_code` (IPv4+IPv6)
- `i2p_router_net_testing` / `i2p_router_net_testing_v6`
- `i2p_router_tunnels_participating`, `_inbound`, `_outbound`, `_queue`, `_tbmqueue`, `_success_ratio`, `_total_success_ratio`
- `i2p_router_netdb_activepeers`, `_knownpeers`, `_floodfills`, `_leasesets`
- `i2p_router_net_bytes_total{direction}` (`inbound`,`outbound`,`transit`)

**Exporter:**

- `i2pd_exporter_build_info{version}`
- `i2pd_exporter_scrape_duration_seconds`
- `i2pd_exporter_effective_scrape_timeout_seconds`
- `i2pd_exporter_last_scrape_error`

---

## Development

`pre-commit` runs the project checks locally:

- `cargo fmt --all -- --check`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets --all-features`

Install the git hook with `pre-commit install`. Run everything manually with `pre-commit run --all-files`.

---

## Releases

GitHub Releases include prebuilt Linux archives for **`x86_64-unknown-linux-gnu`** and **`aarch64-unknown-linux-gnu`**, plus `sha256sums.txt`.

---

## License

# [MIT](./LICENSE)
