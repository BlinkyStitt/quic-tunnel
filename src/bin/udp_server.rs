use argh::FromArgs;
use quic_tunnel::counters::TunnelCounters;
use quic_tunnel::log::configure_logging;
use quic_tunnel::quic::{build_server_endpoint, CongestionMode};
use quinn::Connecting;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::UdpSocket;
use tokio::select;
use tracing::{debug, error, info, trace};

/// Run the QUIC Tunnel Server.
///
/// For improving connections with packet loss, this is the process that runs on the WireGuard server.
///
/// TODO: I don't like the name "Server"
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
        false,
    )?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    let counts = TunnelCounters::new();

    let mut tunnel_handle = {
        let endpoint = endpoint.clone();
        let addr_b = command.remote_addr;

        tokio::spawn(async move {
            while let Some(conn) = endpoint.accept().await {
                let f = handle_connection(conn, addr_b);

                // spawn to handle multiple connections at once
                tokio::spawn(async move {
                    if let Err(e) = f.await {
                        debug!("connection closed: {}", e)
                    }
                });
            }
        })
    };

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

    endpoint.close(0u32.into(), b"server done");

    Ok(())
}

async fn handle_connection(conn_a: Connecting, addr_b: SocketAddr) -> anyhow::Result<()> {
    // let conn_a = match conn_a.into_0rtt() {
    //     Ok((conn, _)) => conn,
    //     Err(conn) => conn.await?,
    // };

    let conn_a = conn_a.await?;

    // TODO: look at the handshake data to figure out what client connected. that way we know what TcpListener to connect it to
    // conn.handshake_data()

    loop {
        // each new QUIC stream gets a new UDP socket
        let stream_a = conn_a.accept_bi().await;

        // TODO: bind ipv4 or 6?
        let socket_b = UdpSocket::bind("0.0.0.0:0").await?;
        socket_b.connect(addr_b).await?;

        let socket_b = Arc::new(socket_b);

        let (tx_a, rx_a) = match stream_a {
            Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                debug!("connection closed");
                return Ok(());
            }
            Err(e) => {
                return Err(e.into());
            }
            Ok(s) => s,
        };

        let f = handle_request(tx_a, rx_a, socket_b);

        // spawn to handle multiple requests at once
        tokio::spawn(async move {
            if let Err(e) = f.await {
                error!("failed: {reason}", reason = e.to_string());
            }
        });
    }
}

/// TODO: counters
/// TODO: i think if we use UdpFramed, we can do tokio::io::copy or tokio::io::copy_bidirectional
async fn handle_request(
    mut tx_a: quinn::SendStream,
    mut rx_a: quinn::RecvStream,
    socket_b: Arc<UdpSocket>,
) -> anyhow::Result<()> {
    // listen on rx. when anything arrives, forward it to socket_b
    let read_f = {
        let socket_b = socket_b.clone();

        async move {
            // let max_size = rx_a.max_datagram_size().unwrap_or(8096);
            let max_size = 8096;

            let mut buf = Vec::with_capacity(max_size);

            loop {
                let n = rx_a.read_buf(&mut buf).await?;

                trace!("rx_a -> socket_b = {}", n);

                socket_b.send(&buf[..n]).await?;
            }
        }
    };
    // TODO: log errors and return ()
    let mut read_f: tokio::task::JoinHandle<anyhow::Result<()>> = tokio::spawn(read_f);

    let write_f = async move {
        loop {
            socket_b.readable().await?;

            let mut buf = [0; 8096];

            match socket_b.recv(&mut buf).await {
                Ok(n) => {
                    trace!("socket_b -> tx_a = {}", n);

                    tx_a.write_all(&buf[..n]).await?;
                }
                Err(e) => {
                    error!("failed to read from socket: {}", e);
                    break;
                }
            }
        }

        Ok(())
    };

    // TODO: log errors and return ()
    let mut write_f: tokio::task::JoinHandle<anyhow::Result<()>> = tokio::spawn(write_f);

    select! {
        x = &mut read_f => {
            trace!("read_f finished: {:?}", x);
        }
        x = &mut write_f => {
            trace!("write_f finished: {:?}", x);
        }
    }

    read_f.abort();
    write_f.abort();

    info!("request finished");

    Ok(())
}
