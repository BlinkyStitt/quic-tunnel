use std::{net::SocketAddr, sync::Arc, time::Duration};

use moka::future::Cache;
use strum::EnumString;
use tokio::sync::Mutex;

pub mod counters;
pub mod log;
pub mod quic;
pub mod tls;

#[derive(Default, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum TunnelMode {
    Tcp,
    #[default]
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TunnelCacheKey {
    pub addr_a: SocketAddr,
    pub from: SocketAddr,
    pub addr_b: SocketAddr,
}

pub type TunnelCache = Cache<
    TunnelCacheKey,
    (
        Arc<Mutex<quinn::SendStream>>,
        Arc<Mutex<Option<quinn::RecvStream>>>,
    ),
>;

/// how long to wait for a tunnel to be idle before closing it.
pub fn get_default_timeout() -> Duration {
    Duration::from_secs(30)
}
