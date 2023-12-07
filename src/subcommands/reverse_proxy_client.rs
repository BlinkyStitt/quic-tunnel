use argh::FromArgs;
use futures::TryFutureExt;
use quic_tunnel::{
    compress::{copy_bidirectional_with_compression, CompressAlgo},
    quic::{build_client_endpoint, CongestionMode},
    stream::Stream,
};
use std::{net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{
    net::{TcpSocket, UnixStream},
    time::timeout,
};
use tracing::{debug, info, trace};

#[derive(Debug, FromArgs, PartialEq)]
/// Run the QUIC Tunnel Client for forwarding a TCP port.
#[argh(subcommand, name = "reverse_proxy_client")]
pub struct ReverseProxyClientSubCommand {
    /// prefix for all the certificates to load
    #[argh(positional)]
    cert_name: String,

    /// the address of the remote QUIC server
    #[argh(positional)]
    remote_quic_addr: SocketAddr,

    /// the address of the nearby service to forward
    #[argh(option)]
    tcp_connect: Option<SocketAddr>,

    /// the socket path of the nearby service to forward
    #[argh(option)]
    unix_connect: Option<PathBuf>,

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

impl ReverseProxyClientSubCommand {
    pub async fn main(self) -> anyhow::Result<()> {
        if self.tcp_connect.is_none() ^ self.unix_connect.is_none() {
            anyhow::bail!("specify either tcp_connect or socket_connect. not none. not both");
        }

        let ca = PathBuf::new().join(format!("{}_ca.pem", self.cert_name));
        let cert = PathBuf::new().join(format!("{}_client.pem", self.cert_name));
        let key = PathBuf::new().join(format!("{}_client.key.pem", self.cert_name));

        // connect to the QUIC endpoint on the server
        // since the client initiates the connections, the client needs keep alive
        let endpoint = build_client_endpoint(ca, cert.clone(), key, self.congestion_mode, true)?;

        let remote_name = self.remote_name.unwrap_or_else(|| {
            // TODO: read the cert and use the name on it rather than the filename. filename works for our dev certs though so its fine for now
            let client_name = cert.file_stem().unwrap().to_string_lossy().to_string();

            client_name.replace("client", "server")
        });

        // TODO: how should we handle reconnecting?
        let remote = endpoint.connect(self.remote_quic_addr, &remote_name)?;

        let remote = match remote.into_0rtt() {
            Ok((remote, _)) => {
                trace!("0-rtt accepted");
                remote
            }
            Err(remote) => timeout(Duration::from_secs(30), remote).await??,
        };

        info!("connected to QUIC server at {}", remote.remote_address());

        loop {
            // TODO: connection pool for re-using these streams
            let stream = if let Some(tcp_connect) = self.tcp_connect {
                let tcp_socket = TcpSocket::new_v4()?;

                trace!(?tcp_socket, "new socket for {}", tcp_connect);

                let nearby_tcp_stream = tcp_socket.connect(tcp_connect).await?;

                debug!(
                    "connected to nearby tcp server at {}",
                    nearby_tcp_stream.peer_addr().unwrap()
                );

                Stream::Tcp(nearby_tcp_stream)
            } else if let Some(unix_connect) = &self.unix_connect {
                debug!("connecting to unix socket at {}", unix_connect.display());

                let unix_stream = UnixStream::connect(unix_connect).await?;

                Stream::Unix(unix_stream)
            } else {
                unimplemented!();
            };

            let (remote_tx, remote_rx) = remote.accept_bi().await?;

            debug!("reverse proxy server connected to us");

            let f =
                copy_bidirectional_with_compression(self.compress, remote_rx, remote_tx, stream);

            tokio::spawn(f.inspect_err(|err| debug!(?err, "reverse proxy client error")));
        }
    }
}
