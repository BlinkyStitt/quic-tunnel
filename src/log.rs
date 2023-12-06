use tracing::{info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// TODO: filtered fmt layer, plus a different filter for tokio-console
/// TODO: verbosity options from command
/// TODO: better way of setting defaults
/// TODO: sentry
/// TODO: panic handler
pub fn configure_logging() {
    tracing_subscriber::registry()
        .with(fmt::layer().pretty())
        .with(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

    info!("hello, world!");
}
