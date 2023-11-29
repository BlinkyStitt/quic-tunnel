use argh::FromArgs;
use quic_tunnel::quic::build_client_endpoint;
use std::path::PathBuf;
use tracing::info;

#[derive(FromArgs)]
/// Run the QUIC Tunnel Client
struct Client {
    #[argh(positional)]
    ca: PathBuf,

    #[argh(positional)]
    cert: PathBuf,

    #[argh(positional)]
    key: PathBuf,

    /// the local address to listen on
    #[argh(positional)]
    local_addr: String,

    /// the remote address to connect to
    #[argh(positional)]
    remote_addr: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Client = argh::from_env();

    let endpoint = build_client_endpoint(command.ca, command.cert, command.key)?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    Ok(())
}
