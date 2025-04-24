# i2pd‑exporter

A **tiny, pure‑Rust** Prometheus exporter that surfaces metrics from the i2pd _I2PControl_ JSON‑RPC API.

---

## Highlights

- Polls I2PControl and serves metrics on **:9600** (configurable).
- Negotiates API tokens automatically.
- Negligible memory & CPU footprint.
- Metrics cover router status, uptime, bandwidth, peers, tunnels and exporter version.

---

## Quick start

```bash
cargo build --release               # native build
./target/release/i2pd-exporter --version # Check version
./target/release/i2pd-exporter      # Run the exporter
```

### Static Linux (Docker)

```bash
./build-static-linux-docker.sh      # outputs to ./dist/
```

---

## Releases

GitHub releases include pre-compiled static Linux binaries (`.tar.gz`) for `x86_64` and `aarch64`. Each release also provides a `sha256sums.txt` file for verifying archive integrity.

---

## Configuration

| Variable               | Default                  | Purpose                                    |
| ---------------------- | ------------------------ | ------------------------------------------ |
| `I2PCONTROL_ADDRESS`   | `https://127.0.0.1:7650` | I2PControl endpoint (`/jsonrpc` appended)  |
| `I2PCONTROL_PASSWORD`  | `itoopie`                | I2PControl password **(required)**         |
| `METRICS_LISTEN_ADDR`  | `0.0.0.0:9600`           | Address:port for metrics (9446 in Ansible) |
| `HTTP_TIMEOUT_SECONDS` | `60`                     | API request timeout (seconds)              |

---

## Metrics cheat‑sheet

- `i2p_router_status`
- `i2p_router_version_info{version}`
- `i2p_router_uptime_seconds`
- `i2p_router_bandwidth_inbound_bytes_per_second{interval}`
- `i2p_router_bandwidth_outbound_bytes_per_second{interval}`
- `i2p_router_network_status_code`
- `i2p_router_tunnels_participating`
- `i2p_router_netdb_activepeers`
- `i2p_router_netdb_knownpeers`
- `i2pd_exporter_version_info{version}`

---

## systemd unit (example)

```ini
[Unit]
Description=I2Pd Control Metrics Exporter
Requires=i2pd.service
After=i2pd.service

[Service]
Type=simple
ExecStart=/usr/local/bin/i2pd-exporter
Environment="I2PCONTROL_ADDRESS=https://127.0.0.1:7650"
Environment="I2PCONTROL_PASSWORD=YOUR_I2PD_CONTROL_PASSWORD"
Environment="METRICS_LISTEN_ADDR=0.0.0.0:9446"
Environment="RUST_LOG=info"
Restart=on-failure
RestartSec=10
User=i2pd
Group=i2pd

[Install]
WantedBy=multi-user.target
```

Enable and launch:

```bash
sudo systemctl enable i2pd-exporter.service
sudo systemctl start i2pd-exporter.service
sudo systemctl status i2pd-exporter.service
```
