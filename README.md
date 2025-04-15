# i2pd-exporter

A Prometheus exporter for i2pd, written in Rust.

## Features

- Collects metrics from i2pd via its **I2PControl JSON-RPC API** (HTTPS).
- Uses the `Authenticate` and `RouterInfo` API methods.
- Handles API token acquisition and automatic refresh on expiry.
- Exposes metrics in Prometheus format (default port: 9600, configured to 9446 by Ansible).
- Low memory footprint and efficient performance.
- **Note:** Connects via HTTPS and explicitly accepts self-signed certificate used by i2pd's I2PControl interface.

## Building

The primary way to build the exporter is using the standard Rust toolchain:

```bash
cargo build --release
```

The resulting binary will be located at `target/release/i2pd-exporter`.

### Building Static Linux Binary (via Docker)

For convenience, a script is provided to build a static Linux binary suitable for deployment on `x86_64-unknown-linux-gnu` targets using Docker:

1. Ensure Docker is installed and running.
2. Run the build script:

```bash
./build-static-linux-docker.sh
```

This script builds the binary using a Docker container and copies the compiled static binary to the `./dist/` directory within this project.

## Configuration

The exporter is configured through environment variables:

- `I2PCONTROL_ADDRESS`: URL of the i2pd I2PControl JSON-RPC endpoint (default: "https://127.0.0.1:7650"). The exporter appends `/jsonrpc` automatically.
- `I2PCONTROL_PASSWORD`: Password for the I2PControl API (default: "itoopie"). Required for authentication.
- `METRICS_LISTEN_ADDR`: Address and port for the exporter to listen on (default: "0.0.0.0:9600").
  - Note: When deployed via the associated Ansible role, this is typically set to `0.0.0.0:9446`.
- `HTTP_TIMEOUT_SECONDS`: Timeout for HTTP requests to the I2PControl API in seconds (default: 60).

## Metrics

The exporter provides the following metrics based on the `RouterInfo` API call:

- `i2p_router_status`: Router status string (numeric representation, e.g., 1.0 for "OK")
- `i2p_router_version_info{version="..."}`: Router version information (gauge=1)
- `i2p_router_uptime_seconds`: Router uptime in seconds
- `i2p_router_bandwidth_inbound_bytes_per_second{interval="1s|15s"}`: Network bandwidth inbound
- `i2p_router_bandwidth_outbound_bytes_per_second{interval="1s|15s"}`: Network bandwidth outbound
- `i2p_router_network_status_code`: Current router network status code (numeric)
- `i2p_router_tunnels_participating`: Number of participating tunnels
- `i2p_router_netdb_activepeers`: Number of active peers
- `i2p_router_netdb_fastpeers`: Number of fast peers
- `i2p_router_netdb_highcapacitypeers`: Number of high capacity peers
- `i2p_router_netdb_is_reseeding`: Whether the router is currently reseeding (1=yes, 0=no)
- `i2p_router_netdb_knownpeers`: Number of known peers
- `i2pd_exporter_version_info{version="..."}`: Version of the i2pd-exporter itself (gauge=1)

## Deployment

The compiled binary can be found in the `./dist/` directory after running the `build-static-linux-docker.sh` script, or in `target/release/` after a native build.

Deployment instructions depend on your specific environment. You will typically need to:

1. Copy the binary to your target server.
2. Configure it to run as a service (e.g., using systemd).
3. Provide the necessary environment variables for configuration (see Configuration section above).
