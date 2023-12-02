use tracing::{info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn configure_logging() {
    // TODO: filtered fmt layer, plus a different filter for tokio-console
    // TODO: verbosity options from command
    // TODO: better way of setting defaults
    tracing_subscriber::registry()
        .with(fmt::layer().pretty())
        .with(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("hello, world!");
}
