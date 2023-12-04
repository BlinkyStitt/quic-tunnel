use argh::FromArgs;
use futures::TryFutureExt;
use quic_tunnel::{
    compress::{copy_bidirectional_with_compression, CompressAlgo},
    log::configure_logging,
    quic::{build_client_endpoint, CongestionMode},
};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{net::TcpSocket, time::timeout};
use tracing::{debug, info, trace};

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

    /// compression mode for the QUIC tunnel.
    ///
    /// Be very careful with this! See: [CRIME](https://en.wikipedia.org/wiki/CRIME) attack!
    #[argh(option, default = "CompressAlgo::None")]
    compress: CompressAlgo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: ReverseProxyClientCommand = argh::from_env();

    configure_logging();

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
        let client_name = command
            .cert
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();

        client_name.replace("client", "server")
    });

    let remote = endpoint.connect(command.remote_quic_addr, &remote_name)?;

    let remote = match remote.into_0rtt() {
        Ok((remote, _)) => remote,
        Err(remote) => timeout(Duration::from_secs(30), remote).await??,
    };

    info!("connected to QUIC server at {}", remote.remote_address());

    loop {
        let tcp_socket = TcpSocket::new_v4()?;

        trace!(?tcp_socket, "new socket for {}", command.nearby_tcp_addr);

        let nearby_tcp_stream = tcp_socket.connect(command.nearby_tcp_addr).await?;

        debug!(
            "connected to nearby tcp server at {}",
            nearby_tcp_stream.peer_addr().unwrap()
        );

        let (remote_tx, remote_rx) = remote.accept_bi().await?;

        debug!("reverse proxy server connected to us");

        let f = copy_bidirectional_with_compression(
            command.compress,
            remote_rx,
            remote_tx,
            nearby_tcp_stream,
        );

        tokio::spawn(f.inspect_err(|err| debug!(?err, "reverse proxy client error")));
    }
}
