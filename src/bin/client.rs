use anyhow::Context;
use argh::FromArgs;
use moka::future::CacheBuilder;
use quic_tunnel::{
    counters::TunnelCounters, get_default_timeout, log::configure_logging,
    quic::build_client_endpoint, TunnelCache, TunnelCacheKey, TunnelMode,
};
use quinn::Connection;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{net::UdpSocket, select, sync::Mutex};
use tracing::{debug, error, info, trace};

#[derive(FromArgs)]
/// Run the QUIC Tunnel Client.
/// TODO: this should be toml config
struct Client {
    #[argh(positional)]
    cert_chain: PathBuf,

    #[argh(positional)]
    key: PathBuf,

    /// the local address to listen on
    #[argh(positional)]
    local_addr: SocketAddr,

    /// the remote server to connect to
    #[argh(positional)]
    remote_addr: SocketAddr,

    #[argh(positional)]
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
    let endpoint = build_client_endpoint(command.cert_chain, command.key)?;

    let remote = endpoint
        .connect(command.remote_addr, &command.remote_name)?
        .await?;

    info!("QUIC listening on {:?}", remote.local_ip());

    let counts = TunnelCounters::new();

    let timeout = get_default_timeout();

    // TODO: i think we want to fill this with something from the QUIC endpoint
    // TODO: what cache timeout?
    let cache: TunnelCache = CacheBuilder::new(10_000).time_to_idle(timeout * 2).build();

    // listen on UDP or TCP
    let mut client_handle = match command.tunnel_mode {
        TunnelMode::Tcp => todo!("TCP"),
        TunnelMode::Udp => {
            let local_socket = UdpSocket::bind(command.local_addr).await?;

            trace!(?local_socket);

            let local_socket = Arc::new(local_socket);

            let counts = counts.clone();

            tokio::spawn(forward_sock_to_endpoint(
                local_socket,
                remote,
                cache,
                counts,
            ))
        }
    };

    let mut stats_handle = counts.spawn_stats_loop();

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

    Ok(())
}

/// copy things on socket to endpoint and save the from address.
/// then spawn a task that reads from the endpoint and sends everything to socket_a and the saved from address.
async fn forward_sock_to_endpoint(
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
