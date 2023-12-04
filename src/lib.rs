use std::{net::SocketAddr, sync::Arc, time::Duration};

use moka::future::Cache;
use tokio::sync::Mutex;

pub mod certs;
pub mod counters;
pub mod log;
pub mod quic;
pub mod tls;

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
    Duration::from_secs(300)
}
