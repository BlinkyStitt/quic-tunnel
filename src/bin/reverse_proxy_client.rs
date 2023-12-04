use argh::FromArgs;
use quic_tunnel::{
    log::configure_logging,
    quic::{build_client_endpoint, CongestionMode},
};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{io::copy_bidirectional, net::TcpSocket, time::timeout};
use tokio_duplex::Duplex;
use tracing::info;

#[derive(FromArgs)]
/// Run the QUIC Tunnel Client for forwarding a TCP port.
struct ReverseProxyClientCommand {
    /// CA certificate in PEM format
    #[argh(positional)]
    ca: PathBuf,

    /// TLS certificate in PEM format
    #[argh(positional)]
    cert: PathBuf,

    /// TLS private key in PEM format
    #[argh(positional)]
    key: PathBuf,

    /// the address of the nearby service to forward
    #[argh(positional)]
    nearby_tcp_addr: SocketAddr,

    /// the address of the remote QUIC server
    #[argh(positional)]
    remote_quic_addr: SocketAddr,

    /// the name on the remote server's certificate.
    ///
    /// If not specified, will be calculated based on `cert`.
    #[argh(option)]
    remote_name: Option<String>,

    /// congestion mode for QUIC
    #[argh(option, default = "Default::default()")]
    congestion_mode: CongestionMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: ReverseProxyClientCommand = argh::from_env();

    configure_logging();

    // connect to the nearby tcp address
    // TODO: prompt ipv4 or ipv6 or both
    let tcp_socket = TcpSocket::new_v4()?;

    let mut nearby_tcp_stream = tcp_socket.connect(command.nearby_tcp_addr).await?;

    info!(
        "connected to nearby tcp server at {}",
        nearby_tcp_stream.peer_addr().unwrap()
    );

    // connect to the QUIC endpoint on the server
    // since the client initiates the connections, the client needs keep alive
    let endpoint = build_client_endpoint(
        command.ca,
        command.cert.clone(),
        command.key,
        command.congestion_mode,
        true,
    )?;

    let remote_name = command.remote_name.unwrap_or_else(|| {
        // TODO: read the cert and use the name on it rather than the filename. filename works for our dev certs though so its fine for now
        command
            .cert
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });

    let remote = endpoint.connect(command.remote_quic_addr, &remote_name)?;

    let remote = match remote.into_0rtt() {
        Ok((remote, _)) => remote,
        Err(remote) => timeout(Duration::from_secs(30), remote).await??,
    };

    info!("connected to QUIC server at {}", remote.remote_address());

    let (remote_tx, remote_rx) = remote.accept_bi().await?;

    let mut remote_stream = Duplex::new(remote_rx, remote_tx);

    copy_bidirectional(&mut nearby_tcp_stream, &mut remote_stream).await?;

    info!("tunnel closed");

    Ok(())
}
