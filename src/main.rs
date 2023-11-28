use anyhow::Context;
use argh::FromArgs;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{
    io,
    net::{lookup_host, UdpSocket},
    select,
};
use tracing::{debug, info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(FromArgs)]
/// Reach new heights.
struct QuicTunnel {
    /// the local address to listen on
    #[argh(positional)]
    local_addr: String,

    /// the remote address to connect to
    #[argh(positional)]
    remote_addr: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: QuicTunnel = argh::from_env();

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

    let mut local_handle = tokio::spawn(forward_sock(local_socket, remote_addr, remote_bind));

    select! {
        x = &mut local_handle => {
            info!(?x, "local task finished");
            // remote_handle.abort();
        }
        // x = &mut remote_handle => {
        //     info!(?x, "remote task finished");
        //     local_handle.abort();
        // }
    }

    Ok(())
}

/// copy things on socket_a to socket_b and save the from address.
/// then spawn a task that reads from socket_b and sends everything to socket_a and the saved from address.
async fn forward_sock(
    socket_a: Arc<UdpSocket>,
    addr_b: SocketAddr,
    bind_b: &str,
) -> tokio::io::Result<()> {
    loop {
        socket_a.readable().await?;

        // The buffer is **not** included in the async task and will only exist on the stack.
        // TODO: what size should this buffer be?
        let mut data = [0; 1024];

        match socket_a.try_recv_from(&mut data[..]) {
            Ok((n, from)) => {
                debug!(sock=%socket_a.local_addr().unwrap(), "received {n} bytes from {from}");

                // TODO: don't bind every time, bind based on from
                let socket_b = UdpSocket::bind(bind_b).await?;
                socket_b.connect(addr_b).await?;

                // TODO: local and remote send different places. need to include from somehow. i think we need a new socket every time which feels wrong
                socket_b.send(&data[..n]).await?;

                let socket_a = socket_a.clone();

                // wait for socket_b to receive something or close
                tokio::spawn(async move {
                    loop {
                        socket_b.readable().await?;

                        let mut data = [0; 1024];

                        match socket_b.try_recv(&mut data[..]) {
                            Ok(n) => {
                                debug!(sock=%socket_b.local_addr().unwrap(), "received {n} bytes from {from}");

                                socket_a.send_to(&data[..n], from).await?;
                            }
                            Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                                // False-positive, continue
                            }
                            Err(e) => {
                                // Actual error. Return it
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
                return Err(e);
            }
        }
    }
}
