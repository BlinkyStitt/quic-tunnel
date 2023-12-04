use argh::FromArgs;
use flume::Receiver;
use futures::TryFutureExt;
use quic_tunnel::counters::TunnelCounters;
use quic_tunnel::log::configure_logging;
use quic_tunnel::quic::{build_server_endpoint, CongestionMode};
use quinn::Connecting;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::select;
use tokio_duplex::Duplex;
use tracing::{error, info, trace};

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
    ///
    /// TODO: descriptive name
    #[argh(positional)]
    quic_addr: SocketAddr,

    /// the TCP address to bind. users that connect here will be forwarded to any clients connected to the QUIC address.
    ///
    /// TODO: descriptive name
    #[argh(positional)]
    tcp_addr: SocketAddr,

    /// congestion mode for QUIC
    #[argh(option, default = "CongestionMode::NewReno")]
    congestion_mode: CongestionMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Server = argh::from_env();

    configure_logging();

    // TODO: bounded channel
    let (tcp_tx, tcp_rx) = flume::unbounded();

    let endpoint = build_server_endpoint(
        command.ca,
        command.cert,
        command.key,
        true,
        command.quic_addr,
        command.congestion_mode,
        false,
    )?;

    info!("QUIC listening on {}", endpoint.local_addr()?);

    let counts = TunnelCounters::new();

    // the tunnel handle listens on quic and forwards messages from a channel for tcp
    // TODO: better name
    let mut quic_endpoint_handle = {
        let endpoint = endpoint.clone();
        let tcp_rx = tcp_rx.clone();

        let f = async move {
            while let Some(conn) = endpoint.accept().await {
                let f = handle_quic_connection(conn, tcp_rx.clone());

                // spawn to handle multiple connections at once? we only have one listener right now
                tokio::spawn(f.inspect_err(|err| trace!(?err, "reverse proxy tunnel closed")));
            }
        };

        // this handle isn't needed. errors are logged elsewhere
        tokio::spawn(f)
    };

    // listens on tcp and forward all connections through a channel. any clients connected over quic will read the channel and handle the tcp stream
    let mut tcp_listener_handle: tokio::task::JoinHandle<Result<(), anyhow::Error>> = {
        let f = async move {
            // TODO: wait until at least one client has connected to the quic endpoint?

            let tcp_listener = TcpListener::bind(command.tcp_addr).await?;
            info!("TCP listening on {}", tcp_listener.local_addr()?);

            loop {
                match tcp_listener.accept().await {
                    Ok((stream, _)) => {
                        // send the stream to a channel. one of multiple connections might handle it
                        tcp_tx.send_async(stream).await?
                    }
                    Err(err) => error!(?err, "tcp accept failed"),
                }
            }
        };

        tokio::spawn(f.inspect_err(|err| trace!(?err, "tcp listener proxy closed")))
    };

    let mut stats_handle = counts.spawn_stats_loop();

    select! {
        x = &mut quic_endpoint_handle => {
            info!(?x, "tunnel task finished");
        }
        x = &mut tcp_listener_handle => {
            info!(?x, "proxy task finished");
        }
        x = &mut stats_handle => {
            info!(?x, "stats task finished");
        }
    }

    tcp_listener_handle.abort();
    quic_endpoint_handle.abort();
    stats_handle.abort();

    endpoint.close(0u32.into(), b"server done");

    Ok(())
}

async fn handle_quic_connection(
    conn_a: Connecting,
    rx_b: Receiver<TcpStream>,
) -> anyhow::Result<()> {
    // TODO: are there other things I need to do to set up 0-rtt?
    let conn_a = match conn_a.into_0rtt() {
        Ok((conn, _)) => conn,
        Err(conn) => conn.await?,
    };

    // TODO: look at the handshake data to figure out what client connected. that way we know what TcpListener to connect it to?

    loop {
        while let Ok(mut stream_b) = rx_b.recv_async().await {
            // each new TCP stream gets a new QUIC stream
            let (tx_a, rx_a) = conn_a.accept_bi().await?;

            let mut stream_a = Duplex::new(rx_a, tx_a);

            // TODO: counters while the stream happens
            let f = async move { copy_bidirectional(&mut stream_a, &mut stream_b).await };

            // spawn to handle multiple requests at once
            tokio::spawn(f.inspect_err(|e| {
                error!("failed: {reason}", reason = e.to_string());
            }));
        }
    }
}
