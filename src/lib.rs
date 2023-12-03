use std::{net::SocketAddr, sync::Arc, time::Duration};

use moka::future::Cache;
use strum::EnumString;
use tokio::sync::Mutex;

pub mod certs;
pub mod counters;
pub mod log;
pub mod quic;
pub mod tls;

/// TODO: forward unix sockets
/// TODO: forward from a tcp socket to a unix socket
#[derive(Default, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum TunnelMode {
    /// Forward traffic through a port on the server to a port accessible on the client.
    TcpReverseProxy,
    /// Forward UDP traffic.
    #[default]
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TunnelCacheKey {
    pub addr_a: SocketAddr,
    pub from: SocketAddr,
    pub addr_b: SocketAddr,
}

/// Since UDP is stateless, we need to keep track of destinations so we can send responses to the right stream.
pub type TunnelCache = Cache<
    TunnelCacheKey,
    (
        Arc<Mutex<quinn::SendStream>>,
        Arc<Mutex<Option<quinn::RecvStream>>>,
    ),
>;

/// how long to wait for a tunnel to be idle before closing it.
/// TODO: make sure this matches quinn's config.
pub fn get_tunnel_timeout() -> Duration {
    Duration::from_secs(60)
}
