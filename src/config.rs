use clap::Parser;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(
        long,
        env = "I2PCONTROL_ADDRESS",
        default_value = "https://127.0.0.1:7650",
        help = "I2PControl endpoint (without /jsonrpc)"
    )]
    pub i2pcontrol_address: String,

    #[arg(
        long,
        env = "I2PCONTROL_PASSWORD",
        default_value = "itoopie",
        help = "Password for I2PControl API"
    )]
    pub i2pcontrol_password: String,

    #[arg(
        long,
        env = "METRICS_LISTEN_ADDR",
        default_value = "0.0.0.0:9600",
        help = "Address:port for metrics HTTP server"
    )]
    pub metrics_listen_addr: String,

    #[arg(
        long,
        env = "MAX_SCRAPE_TIMEOUT_SECONDS",
        default_value_t = 120u64,
        help = "Hard cap for header-derived scrape timeout (seconds)"
    )]
    pub max_scrape_timeout_seconds: u64,

    #[arg(
        long,
        env = "I2PCONTROL_TLS_INSECURE",
        default_value_t = false,
        help = "Accept invalid TLS certs (not recommended)"
    )]
    pub i2pcontrol_tls_insecure: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub i2p_addr: String,
    pub i2p_password: String,
    pub listen_addr: SocketAddr,
    pub tls_insecure: bool,
    pub max_scrape_timeout: Duration,
}

impl TryFrom<Cli> for Config {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn try_from(cli: Cli) -> Result<Self, Self::Error> {
        let listen_addr: SocketAddr = cli.metrics_listen_addr.parse().map_err(|e| {
            format!(
                "Invalid METRICS_LISTEN_ADDR '{}': {} (expected host:port)",
                cli.metrics_listen_addr, e
            )
        })?;

        Ok(Config {
            i2p_addr: cli.i2pcontrol_address,
            i2p_password: cli.i2pcontrol_password,
            listen_addr,
            tls_insecure: cli.i2pcontrol_tls_insecure,
            max_scrape_timeout: Duration::from_secs(cli.max_scrape_timeout_seconds),
        })
    }
}
