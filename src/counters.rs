use std::fmt::Debug;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tracing::info;

#[derive(Default)]
pub struct TunnelCounters {
    packets_sent: AtomicUsize,
    packets_recv: AtomicUsize,
    bytes_sent: AtomicUsize,
    bytes_recv: AtomicUsize,
}

impl TunnelCounters {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
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
    pub fn sent(&self, n: usize) {
        self.packets_sent.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_sent.fetch_add(n, atomic::Ordering::SeqCst);

        // TODO: send to a watch channel?
    }

    pub fn recv(&self, n: usize) {
        self.packets_recv.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_recv.fetch_add(n, atomic::Ordering::SeqCst);

        // TODO: send to a watch channel?
    }

    pub fn spawn_stats_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let f = async move {
            let mut i = interval(Duration::from_secs(10));
            i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                i.tick().await;

                // TODO: subsctibe to a watch channel. only print counts if they have changed.
                info!(counts=?self, "stats");
            }
        };

        tokio::spawn(f)
    }
}
