use std::fmt::Debug;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::interval;
use tracing::{info, warn};

pub struct TunnelCounters {
    packets_sent: AtomicUsize,
    packets_recv: AtomicUsize,
    bytes_sent: AtomicUsize,
    bytes_recv: AtomicUsize,
    compressed_bytes_sent: AtomicUsize,
    compressed_bytes_recv: AtomicUsize,
    watch: watch::Sender<()>,
}

impl TunnelCounters {
    pub fn new() -> Arc<Self> {
        // there are probably more efficient ways to do this, but it works for now
        let (watch, _) = watch::channel(());

        let data = Self {
            packets_sent: AtomicUsize::new(0),
            packets_recv: AtomicUsize::new(0),
            bytes_sent: AtomicUsize::new(0),
            bytes_recv: AtomicUsize::new(0),
            compressed_bytes_sent: AtomicUsize::new(0),
            compressed_bytes_recv: AtomicUsize::new(0),
            watch,
        };

        Arc::new(data)
    }
}

impl Debug for TunnelCounters {
    /// this doesn't lock the counters, so requests while printing may be missed
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
            "compressed_bytes_sent",
            &self.compressed_bytes_sent.load(atomic::Ordering::SeqCst),
        );

        state.field(
            "packets_recv",
            &self.packets_recv.load(atomic::Ordering::SeqCst),
        );
        state.field(
            "bytes_recv",
            &self.bytes_recv.load(atomic::Ordering::SeqCst),
        );
        state.field(
            "compressed_bytes_recv",
            &self.compressed_bytes_recv.load(atomic::Ordering::SeqCst),
        );

        state.finish()
    }
}

impl TunnelCounters {
    pub fn sent(&self, n: usize, compressed: usize) {
        self.packets_sent.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_sent.fetch_add(n, atomic::Ordering::SeqCst);
        self.compressed_bytes_sent
            .fetch_add(compressed, atomic::Ordering::SeqCst);

        self.watch.send_replace(());
    }

    pub fn recv(&self, n: usize, compressed: usize) {
        self.packets_recv.fetch_add(1, atomic::Ordering::SeqCst);
        self.bytes_recv.fetch_add(n, atomic::Ordering::SeqCst);
        self.compressed_bytes_recv
            .fetch_add(compressed, atomic::Ordering::SeqCst);

        self.watch.send_replace(());
    }

    pub fn spawn_stats_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let mut watch = self.watch.subscribe();
        watch.borrow_and_update();

        let f = async move {
            let mut i = interval(Duration::from_secs(10));
            i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                i.tick().await;

                if let Err(err) = watch.changed().await {
                    warn!("watch channel closed: {}", err);
                    break;
                };

                watch.borrow_and_update();

                // TODO: subsctibe to a watch channel. only print counts if they have changed.
                info!(counts=?self, "stats");
            }
        };

        tokio::spawn(f)
    }
}
