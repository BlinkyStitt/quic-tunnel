use anyhow::Context;
use argh::FromArgs;
use moka::future::CacheBuilder;
use quic_tunnel::{
    counters::TunnelCounters, get_tunnel_timeout, log::configure_logging,
    quic::build_client_endpoint, TunnelCache, TunnelCacheKey, TunnelMode,
};
use quinn::Connection;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, UdpSocket},
    select,
    sync::Mutex,
};
use tracing::{debug, error, info, trace};

#[derive(FromArgs)]
/// Run the QUIC Tunnel Client.
///
/// The Client runs next to the service that you want to forward.
///
/// For personal use on connections with bad packet loss, this is the process that runs on my laptop. It fowards my WireGuard connection.
///
/// For use as a reverse proxy, this is the process that runs on the application servers.
///
/// TODO: I don't like the name "Client"
struct Client {
    /// CA certificate in PEM format
    #[argh(positional)]
    ca: PathBuf,

    /// TLS certificate in PEM format
    #[argh(positional)]
    cert: PathBuf,

    /// TLS private key in PEM format
    #[argh(positional)]
    key: PathBuf,

    /// the local address to listen on
    #[argh(positional)]
    local_addr: SocketAddr,

    /// the remote server to connect to
    #[argh(positional)]
    remote_addr: SocketAddr,

    /// the name on the remote server's certificate
    #[argh(option, default = "\"server\".to_string()")]
    remote_name: String,

    /// tunnel UDP or TCP.
    /// Must match the server
    #[argh(option)]
    tunnel_mode: TunnelMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Client = argh::from_env();

    configure_logging();

    // connect to the remote server
    let endpoint = build_client_endpoint(command.ca, command.cert, command.key)?;

    // TODO: actual server_name instead of assuming we generated simple certs
    let remote = endpoint
        .connect(command.remote_addr, &command.remote_name)?
        .await?;

    info!("QUIC listening on {:?}", remote.local_ip());

    let counts = TunnelCounters::new();

    let timeout = get_tunnel_timeout();

    let cache: TunnelCache = CacheBuilder::new(10_000).time_to_idle(timeout).build();

    // listen on UDP or TCP
    let mut client_handle = match command.tunnel_mode {
        TunnelMode::TcpReverseProxy => {
            let local_socket = TcpListener::bind(command.local_addr).await?;

            trace!(?local_socket);

            let local_socket = Arc::new(local_socket);

            let counts = counts.clone();

            tokio::spawn(forward_tcp_to_endpoint(local_socket, remote, counts))
        }
        TunnelMode::Udp => {
            let local_socket = UdpSocket::bind(command.local_addr).await?;

            trace!(?local_socket);

            let local_socket = Arc::new(local_socket);

            let counts = counts.clone();

            tokio::spawn(forward_udp_to_endpoint(local_socket, remote, cache, counts))
        }
    };

    let mut stats_handle = counts.spawn_stats_loop();

    // TODO: if our network changes, rebind the endpoint to a new udp socket

    select! {
        x = &mut client_handle => {
            info!(?x, "local task finished");
        }
        x = &mut stats_handle => {
            info!(?x, "stats task finished");
        }
    }

    client_handle.abort();
    stats_handle.abort();

    endpoint.close(0u32.into(), b"client done");

    Ok(())
}

/// copy things on socket to endpoint and save the from address.
/// then spawn a task that reads from the endpoint and sends everything to socket_a and the saved from address.
async fn forward_tcp_to_endpoint(
    listener_a: Arc<TcpListener>,
    connection_b: Connection,
    counts: Arc<TunnelCounters>,
) -> anyhow::Result<()> {
    // TODO: spawn something to increment counts somehow?

    loop {
        match listener_a.accept().await {
            Ok((mut stream_a, _from)) => {
                let (tx, rx) = connection_b.open_bi().await?;

                let mut stream_b = tokio_duplex::Duplex::new(rx, tx);

                if let Err(err) = copy_bidirectional(&mut stream_a, &mut stream_b).await {
                    error!("failed during copy: {}", err);
                }
            }
            Err(err) => error!("failed to accept tcp connection: {}", err),
        }
    }
}

/// copy things on socket to endpoint and save the from address.
/// then spawn a task that reads from the endpoint and sends everything to socket_a and the saved from address.
async fn forward_udp_to_endpoint(
    socket_a: Arc<UdpSocket>,
    connection_b: Connection,
    cache: TunnelCache,
    counts: Arc<TunnelCounters>,
) -> anyhow::Result<()> {
    loop {
        socket_a.readable().await?;

        // The buffer is **not** included in the async task and will only exist on the stack.
        // TODO: what size should this buffer be?
        // TODO: do this without allocating
        let mut data = [0; 1024];

        match socket_a.try_recv_from(&mut data) {
            Ok((n, from)) => {
                let addr_a = socket_a.local_addr().unwrap();
                let addr_b = connection_b.remote_address();

                debug!("sending {n} bytes from {from} @ {addr_a:?} over QUIC tunnel to {addr_b}");

                // don't bind every time, re-use existing sockets if they are for the local and remote addresses
                let cache_key = TunnelCacheKey {
                    addr_a,
                    from,
                    addr_b,
                };

                let connection_b = connection_b.clone();

                let (tx, rx) = cache
                    .try_get_with(cache_key, async move {
                        let (tx, rx) = connection_b.open_bi().await?;

                        let tx = Arc::new(Mutex::new(tx));
                        let rx = Arc::new(Mutex::new(Some(rx)));

                        Ok::<_, anyhow::Error>((tx, rx))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("cache error: {}", e))?;

                let mut tx = tx.lock().await;

                // TODO: we don't actually take advantage of quic's multiplexing. this could add the destination address and the server could have a mapping
                // TODO: we would probably want to be able to listen on multiple ports then too
                let tx = tx.write_all(&data[..n]).await;

                match tx {
                    Ok(()) => {
                        counts.sent(n);
                        let socket_a = socket_a.clone();
                        let counts = counts.clone();

                        // we only need to rx once
                        if let Some(mut rx) = rx.lock().await.take() {
                            // wait for socket_b to receive something or close
                            tokio::spawn(async move {
                                // TODO: we need tokio_util::UdpFramed for this
                                // io::copy(&mut rx, &mut socket_a).await?;

                                loop {
                                    // TODO: what should udp timeout be?
                                    // TODO: what should the max size be?
                                    match rx.read_to_end(usize::MAX).await {
                                        Ok(x) => {
                                            debug!("received {n} bytes from {addr_b} for {from} @ {addr_a:?}");

                                            if let Err(e) = socket_a
                                                .send_to(&x, from)
                                                .await
                                                .context("unable to send")
                                            {
                                                error!(
                                                        "error from {addr_b} for {from} @ {addr_a:?}: {e}"
                                                    );
                                                break;
                                            }

                                            counts.recv(n);
                                        }
                                        Err(e) => {
                                            error!(
                                                "error from {addr_b} for {from} @ {addr_a:?}: {e}"
                                            );
                                            break;
                                        }
                                    };
                                }
                            });
                        }
                    }
                    Err(err) => error!("failed to write to QUIC stream: {}", err),
                }
            }
            Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                // False-positive, continue
            }
            Err(e) => {
                // Actual error. Return it
                return Err(e.into());
            }
        }
    }
}
