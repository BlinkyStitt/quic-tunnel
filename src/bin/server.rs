use argh::FromArgs;
use quic_tunnel::quic::build_server_endpoint;
use std::net::SocketAddr;
use std::path::PathBuf;
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Server = argh::from_env();

    let endpoint = build_server_endpoint(
        command.ca,
        command.cert,
        command.key,
        true,
        command.local_addr,
    )?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    while let Some(conn) = endpoint.accept().await {
        info!("connection incoming");

        todo!("wip");
    }

    Ok(())
}
