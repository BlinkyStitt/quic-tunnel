use argh::FromArgs;
use quic_tunnel::counters::TunnelCounters;
use quic_tunnel::log::configure_logging;
use quic_tunnel::quic::build_server_endpoint;
use quic_tunnel::{get_default_timeout, TunnelMode};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::select;
use tracing::info;

#[derive(FromArgs)]
/// Run the QUIC Tunnel Server
struct Server {
    /// TLS private key in PEM format
    #[argh(positional)]
    key: PathBuf,

    /// TLS certificate in PEM format
    #[argh(positional)]
    cert: PathBuf,

    /// CA certificate in PEM format
    #[argh(positional)]
    ca: PathBuf,

    /// the local address to listen on with QUIC.
    #[argh(positional)]
    local_addr: SocketAddr,

    /// the remote address to connect to with UDP
    #[argh(positional)]
    remote_addr: SocketAddr,

    /// tunnel UDP or TCP
    #[argh(option)]
    tunnel_mode: TunnelMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Server = argh::from_env();

    configure_logging();

    let timeout = get_default_timeout();

    let endpoint = build_server_endpoint(
        command.ca,
        command.cert,
        command.key,
        true,
        command.local_addr,
    )?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    let counts = TunnelCounters::new();

    let mut tunnel_handle = tokio::spawn(async move {
        while let Some(conn) = endpoint.accept().await {
            info!("connection incoming");

            todo!("wip");
        }
    });

    let mut stats_handle = counts.spawn_stats_loop();

    select! {
        x = &mut tunnel_handle => {
            info!(?x, "tunnel task finished");
        }
        x = &mut stats_handle => {
            info!(?x, "stats task finished");
        }
    }

    tunnel_handle.abort();
    stats_handle.abort();

    Ok(())
}
