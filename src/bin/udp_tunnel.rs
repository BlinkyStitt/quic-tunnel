use anyhow::Context;
use argh::FromArgs;
use moka::future::{Cache, CacheBuilder};
use std::fmt::Debug;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io,
    net::{lookup_host, UdpSocket},
    select,
    time::timeout,
};
use tracing::{debug, info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(FromArgs)]
/// Run a simple UDP server
struct UdpTunnel {
    /// the local address to listen on with UDP.
    #[argh(positional)]
    local_addr: SocketAddr,

    /// the remote address to connect to with UDP.
    #[argh(positional)]
    remote_addr: SocketAddr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct UdpTunnelKey {
    addr_a: SocketAddr,
    from: SocketAddr,
    addr_b: SocketAddr,
}

#[derive(Default)]
struct TunnelCounters {
    packets_sent: AtomicUsize,
    packets_recv: AtomicUsize,
    bytes_sent: AtomicUsize,
    bytes_recv: AtomicUsize,
}

impl Debug for TunnelCounters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut state = f.debug_struct("TunnelCounters");

        state.field(
            "packets_sent",
            &self.packets_sent.load(atomic::Ordering::SeqCst),
        );
        state.field(
            "bytes_sent",
            &self.bytes_sent.load(atomic::Ordering::SeqCst),
        );

        state.field(
            "packets_recv",
            &self.packets_recv.load(atomic::Ordering::SeqCst),
        );
        state.field(
            "bytes_recv",
            &self.bytes_recv.load(atomic::Ordering::SeqCst),
        );

        state.finish()
    }
}

impl TunnelCounters {
    fn sent(&self, n: usize) {
        self.packets_sent.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_sent.fetch_add(n, atomic::Ordering::SeqCst);
    }

    fn recv(&self, n: usize) {
        self.packets_recv.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_recv.fetch_add(n, atomic::Ordering::SeqCst);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: UdpTunnel = argh::from_env();

    // TODO: filtered fmt layer, plus a different filter for tokio-console
    // TODO: verbosity options from command
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().pretty())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("hello, world!");

    let local_socket = UdpSocket::bind(command.local_addr).await?;

    trace!(?local_socket);

    let remote_addr = lookup_host(command.remote_addr)
        .await?
        .next()
        .context("no remote address")?;

    trace!(?remote_addr);

    let local_addr = local_socket.local_addr()?;

    let remote_bind = if remote_addr.is_ipv4() {
        assert!(local_addr.is_ipv4());

        "0.0.0.0:0"
    } else {
        assert!(local_addr.is_ipv6());

        "[::]:0"
    };

    trace!(?remote_bind);

    // put the sockets inside Arcs so we can share them between tasks
    let local_socket = Arc::new(local_socket);

    let default_timeout = Duration::from_secs(30);

    let counts = Arc::new(TunnelCounters::default());

    let cache = CacheBuilder::new(10_000)
        .time_to_idle(Duration::from_secs(60))
        .build();

    let mut local_handle = tokio::spawn(forward_sock(
        local_socket,
        remote_addr,
        remote_bind,
        cache.clone(),
        counts.clone(),
        default_timeout,
    ));

    let mut stats_handle = tokio::spawn(print_stats(counts));

    select! {
        x = &mut local_handle => {
            info!(?x, "local task finished");
        }
        x = &mut stats_handle => {
            info!(?x, "stats task finished");
        }
    }

    local_handle.abort();
    stats_handle.abort();

    Ok(())
}

/// copy things on socket_a to socket_b and save the from address.
/// then spawn a task that reads from socket_b and sends everything to socket_a and the saved from address.
async fn forward_sock(
    socket_a: Arc<UdpSocket>,
    addr_b: SocketAddr,
    bind_b: &str,
    cache: Cache<UdpTunnelKey, Arc<UdpSocket>>,
    counts: Arc<TunnelCounters>,
    default_timeout: Duration,
) -> anyhow::Result<()> {
    loop {
        socket_a.readable().await?;

        // The buffer is **not** included in the async task and will only exist on the stack.
        // TODO: what size should this buffer be?
        let mut data = [0; 1024];

        match socket_a.try_recv_from(&mut data[..]) {
            Ok((n, from)) => {
                let addr_a = socket_a.local_addr().unwrap();

                debug!("sending {n} bytes from {from} @ {addr_a:?} to {addr_b}");

                let cache_key = UdpTunnelKey {
                    addr_a,
                    from,
                    addr_b,
                };

                // don't bind every time, re-use existing sockets if they are for the local and remote addresses
                let socket_b = cache
                    .try_get_with(cache_key, async move {
                        let socket_b = UdpSocket::bind(bind_b).await?;
                        socket_b.connect(addr_b).await?;

                        Ok::<_, anyhow::Error>(Arc::new(socket_b))
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("cache error: {}", e))?;

                socket_b.send(&data[..n]).await?;
                counts.sent(n);

                let socket_a = socket_a.clone();
                let counts = counts.clone();

                // wait for socket_b to receive something or close
                tokio::spawn(async move {
                    // io::copy(&mut reader, &mut writer).await?;

                    loop {
                        // TODO: what should udp timeout be?
                        match timeout(default_timeout, socket_b.readable()).await {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                debug!(
                                    "no longer readable from {addr_b} for {from} @ {addr_a:?}: {e}"
                                );
                                return Err(e);
                            }
                            Err(e) => {
                                trace!("timeout from {addr_b} for {from} @ {addr_a:?}: {e}");
                                return Ok(());
                            }
                        };

                        let mut data = [0; 1024];

                        match socket_b.try_recv(&mut data[..]) {
                            Ok(n) => {
                                debug!("received {n} bytes from {addr_b} for {from} @ {addr_a:?}");

                                socket_a.send_to(&data[..n], from).await?;
                                counts.recv(n);
                            }
                            Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                                // False-positive, continue
                            }
                            Err(e) => {
                                // return the error
                                debug!("error from {addr_b} for {from} @ {addr_a:?}: {e}");
                                return Err::<(), io::Error>(e);
                            }
                        }
                    }
                });
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

async fn print_stats(counters: Arc<TunnelCounters>) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // TODO: wait for counts to change?
        // TODO: only print counts if they have changed

        info!(?counters, "stats");
    }
}
