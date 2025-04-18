# i2pd-exporter

A Prometheus exporter for i2pd, written in Rust.

## Features

- Collects metrics from i2pd using its I2PControl JSON-RPC API.
- Automatically acquires and refreshes API tokens.
- Exposes metrics in Prometheus format (default port: 9600).
- Efficient and lightweight.

## Building

Build the exporter using the standard Rust toolchain:

```bash
cargo build --release
```

The binary will be located at `target/release/i2pd-exporter`.

### Building Static Linux Binary (via Docker)

A script is provided to build a static Linux binary (`x86_64-unknown-linux-gnu`) using Docker:

1. Ensure Docker is installed and running.
2. Run the build script:

```bash
./build-static-linux-docker.sh
```

This builds the binary in a Docker container and copies the static binary to `./dist/`.

## Configuration

Configure the exporter using environment variables:

- `I2PCONTROL_ADDRESS`: URL of the i2pd I2PControl JSON-RPC endpoint (default: "https://127.0.0.1:7650"). `/jsonrpc` is appended automatically.
- `I2PCONTROL_PASSWORD`: Password for the I2PControl API (default: "itoopie"). Required.
- `METRICS_LISTEN_ADDR`: Address and port for the exporter to listen on (default: "0.0.0.0:9600").
- `HTTP_TIMEOUT_SECONDS`: Timeout for I2PControl API requests in seconds (default: 60).

## Metrics

Provides the following metrics (from the `RouterInfo` API call):

- `i2p_router_status`: Router status (numeric, e.g., 1.0 for "OK")
- `i2p_router_version_info{version="..."}`: Router version (gauge=1)
- `i2p_router_uptime_seconds`: Router uptime in seconds
- `i2p_router_bandwidth_inbound_bytes_per_second{interval="1s|15s"}`: Inbound bandwidth (1s/15s avg)
- `i2p_router_bandwidth_outbound_bytes_per_second{interval="1s|15s"}`: Outbound bandwidth (1s/15s avg)
- `i2p_router_network_status_code`: Network status code
- `i2p_router_tunnels_participating`: Participating tunnels count
- `i2p_router_netdb_activepeers`: Active peers count
- `i2p_router_netdb_knownpeers`: Known peers count
- `i2pd_exporter_version_info{version="..."}`: Exporter version (gauge=1)

## Deployment

Find the compiled binary in `./dist/` (static build) or `target/release/` (native build).

To deploy:

1. Copy the binary to your target server (e.g., `/usr/local/bin/i2pd-exporter`).
2. Configure it to run as a service (e.g., using systemd).
3. Set the necessary environment variables (see Configuration).

### Systemd Service Example

Here is an example systemd service file (`/etc/systemd/system/i2pd-exporter.service`). Adjust paths, user, group, and environment variables as needed.

```ini
[Unit]
Description=I2Pd Metrics Exporter
# Ensure i2pd is started before this service
After=i2pd.service
Requires=i2pd.service

[Service]
Type=simple
# Optional: Add startup delay to ensure i2pd is fully initialized
# ExecStartPre=/bin/sleep 3
ExecStart=/usr/local/bin/i2pd-exporter
# Adjust these environment variables according to your i2pd setup
Environment="I2PCONTROL_ADDRESS=https://127.0.0.1:7650"
Environment="I2PCONTROL_PASSWORD=YOUR_I2PD_CONTROL_PASSWORD"
Environment="METRICS_LISTEN_ADDR=0.0.0.0:9446"
Environment="RUST_LOG=info"
# Improved restart policy with backoff
Restart=on-failure
RestartSec=10
# Run as the i2pd user (or another dedicated user)
User=i2pd
Group=i2pd

[Install]
WantedBy=multi-user.target
```

After creating the file, enable and start the service:

```bash
sudo systemctl enable i2pd-exporter.service
sudo systemctl start i2pd-exporter.service
sudo systemctl status i2pd-exporter.service
```
