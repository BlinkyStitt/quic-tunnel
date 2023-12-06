//! TODO: helper for setting routes so that the WireGuard VPN doesn't try to take over the udp tunnel.
//! TODO: refactor this so that the udp and related cache is inside a single StatefulUdpSomething struct.

use anyhow::Context;
use argh::FromArgs;
use moka::future::CacheBuilder;
use quic_tunnel::{
    counters::TunnelCounters,
    get_tunnel_timeout,
    quic::{build_client_endpoint, CongestionMode},
    TunnelCache, TunnelCacheKey,
};
use quinn::Connection;
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::UdpSocket, select, sync::Mutex, time::timeout};
use tracing::{debug, error, info, trace};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "udp_client")]
/// Run the QUIC Tunnel Client for forwarding UDP traffic.
///
/// For improving connections with packet loss, this is the process that tunnels the WireGuard connection to the server.
pub struct UdpClientSubCommand {
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

    /// congestion mode for QUIC
    #[argh(option, default = "Default::default()")]
    congestion_mode: CongestionMode,
}

impl UdpClientSubCommand {
    pub async fn main(self) -> anyhow::Result<()> {
        // connect to the remote server
        let endpoint =
            build_client_endpoint(self.ca, self.cert, self.key, self.congestion_mode, true)?;

        let connecting = endpoint.connect(self.remote_addr, &self.remote_name)?;

        let remote = timeout(Duration::from_secs(30), connecting).await??;

        // TODO: this connection doesn't seem to have keep alive even though I turned it on in the server endpoint.
        // TODO: if this connection isn't used soon, the

        info!(
            "Forwarding {} through QUIC tunnel at {}",
            self.local_addr,
            remote.remote_address()
        );

        let counts = TunnelCounters::new();

        let timeout = get_tunnel_timeout();

        let cache: TunnelCache = CacheBuilder::new(10_000).time_to_idle(timeout).build();

        // listen on UDP
        let local_socket = UdpSocket::bind(self.local_addr).await?;

        trace!(?local_socket);

        let local_socket = Arc::new(local_socket);

        let mut tunnel_handle = tokio::spawn(tunnel_udp_to_endpoint(
            local_socket,
            remote,
            cache,
            counts.clone(),
        ));

        let mut stats_handle = counts.spawn_stats_loop();

        // TODO: if our network changes, rebind the endpoint to a new udp socket

        select! {
            x = &mut tunnel_handle => {
                info!(?x, "local task finished");
            }
            x = &mut stats_handle => {
                info!(?x, "stats task finished");
            }
        }

        tunnel_handle.abort();
        stats_handle.abort();

        endpoint.close(0u32.into(), b"client done");

        Ok(())
    }
}

/// copy things on socket to endpoint and save the from address.
/// then spawn a task that reads from the endpoint and sends everything to socket_a and the saved from address.
async fn tunnel_udp_to_endpoint(
    socket_a: Arc<UdpSocket>,
    connection_b: Connection,
    cache: TunnelCache,
    counts: Arc<TunnelCounters>,
) -> anyhow::Result<()> {
    loop {
        socket_a.readable().await?;

        let max_size = connection_b.max_datagram_size().unwrap_or(8096);

        // The buffer is **not** included in the async task and will only exist on the stack.
        // TODO: what size should this buffer be?
        // TODO: do this without allocating
        let mut data = Vec::with_capacity(max_size);

        match socket_a.try_recv_buf_from(&mut data) {
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

                let (tx_b, rx_b) = cache
                    .try_get_with(cache_key, async move {
                        let (tx_b, rx_b) = connection_b.open_bi().await?;

                        let tx_b = Arc::new(Mutex::new(tx_b));
                        let rx_b = Arc::new(Mutex::new(Some(rx_b)));

                        Ok::<_, anyhow::Error>((tx_b, rx_b))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("cache error: {}", e))?;

                let mut lock_tx_b = tx_b.lock().await;

                // TODO: we don't actually take advantage of quic's multiplexing. this could add the destination address and the server could have a mapping
                // TODO: we would probably want to be able to listen on multiple ports then too
                let tx = lock_tx_b.write_all(&data[..n]).await;

                drop(lock_tx_b);

                match tx {
                    Ok(()) => {
                        counts.sent(n, 0);
                        let socket_a = socket_a.clone();
                        let counts = counts.clone();

                        // we only need to rx once
                        if let Some(mut rx) = rx_b.lock().await.take() {
                            // wait for socket_b to receive something or close
                            tokio::spawn(async move {
                                // TODO: we need tokio_util::UdpFramed for this
                                // io::copy(&mut rx, &mut socket_a).await?;

                                let mut buf = [0; 8096];

                                loop {
                                    // TODO: what should udp timeout be?
                                    // TODO: what should the max size be?
                                    match rx.read(&mut buf).await {
                                        Ok(Some(n)) => {
                                            debug!("received {n} bytes from {addr_b} for {from} @ {addr_a:?}");

                                            if let Err(e) = socket_a
                                                .send_to(&buf[..n], from)
                                                .await
                                                .context("unable to send")
                                            {
                                                error!(
                                                        "error from {addr_b} for {from} @ {addr_a:?}: {e}"
                                                    );
                                                break;
                                            }

                                            counts.recv(n, 0);
                                        }
                                        Ok(None) => {
                                            trace!("connection closed");
                                            break;
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
