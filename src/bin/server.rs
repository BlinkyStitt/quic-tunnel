use argh::FromArgs;
use moka::future::CacheBuilder;
use quic_tunnel::counters::TunnelCounters;
use quic_tunnel::log::configure_logging;
use quic_tunnel::quic::{build_server_endpoint, CongestionMode};
use quic_tunnel::{get_tunnel_timeout, TunnelCache, TunnelMode};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::select;
use tracing::info;

/// Run the QUIC Tunnel Server.
///
/// For personal use on connections with bad packet loss, this is the process that runs on my WireGuard server.
///
/// For use as a reverse proxy, this is the process that runs in the cloud behind a static anycast IP address.
///
/// TODO: this should be toml config
/// TODO: i think for tcp, i've mixed up the sides for client vs server
#[derive(FromArgs)]
struct Server {
    /// CA certificate in PEM format
    #[argh(positional)]
    ca: PathBuf,

    /// TLS certificate in PEM format
    #[argh(positional)]
    cert: PathBuf,

    /// TLS private key in PEM format
    #[argh(positional)]
    key: PathBuf,

    /// the local address to listen on with QUIC. Clients connect here
    #[argh(positional)]
    local_addr: SocketAddr,

    /// the remote address to forward client data to
    #[argh(positional)]
    remote_addr: SocketAddr,

    /// tunnel UDP or TCP
    #[argh(option, default = "Default::default()")]
    tunnel_mode: TunnelMode,

    /// congestion mode for QUIC
    #[argh(option, default = "Default::default()")]
    congestion_mode: CongestionMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Server = argh::from_env();

    configure_logging();

    let endpoint = build_server_endpoint(
        command.ca,
        command.cert,
        command.key,
        true,
        command.local_addr,
        command.congestion_mode,
    )?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    let counts = TunnelCounters::new();

    let timeout = get_tunnel_timeout();

    let cache: TunnelCache = CacheBuilder::new(10_000).time_to_idle(timeout).build();

    let mut tunnel_handle = tokio::spawn(async move {
        while let Some(conn) = endpoint.accept().await {
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
