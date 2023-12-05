use crate::get_tunnel_timeout;

use super::tls;
use quinn::{congestion, ClientConfig, Endpoint, ServerConfig, TransportConfig};
use std::{
    net::{AddrParseError, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use strum::EnumString;
use tracing::trace;

pub fn matching_bind_address(x: SocketAddr) -> Result<SocketAddr, AddrParseError> {
    let bind = if x.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };

    bind.parse()
}

#[derive(Default, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum CongestionMode {
    /// good for high bandwidth networks
    Brr,
    /// good all around
    Cubic,
    /// good for high loss networks
    #[default]
    NewReno,
}

pub fn build_transport_config(
    keep_alive: bool,
    congestion_mode: CongestionMode,
) -> Arc<TransportConfig> {
    let mut transport_config = TransportConfig::default();

    // uni streams are not needed
    transport_config.max_concurrent_uni_streams(0_u32.into());
    // we want lots of bi streams
    transport_config.max_concurrent_bidi_streams(u16::MAX.into());

    let timeout = get_tunnel_timeout();

    match congestion_mode {
        CongestionMode::Brr => {
            transport_config
                .congestion_controller_factory(Arc::new(congestion::BbrConfig::default()));
        }
        CongestionMode::Cubic => {
            transport_config
                .congestion_controller_factory(Arc::new(congestion::CubicConfig::default()));
        }
        CongestionMode::NewReno => {
            transport_config
                .congestion_controller_factory(Arc::new(congestion::NewRenoConfig::default()));
        }
    }

    if keep_alive {
        // only one side needs keep alive
        // TODO: how many keep alives per idle timeout?
        transport_config.keep_alive_interval(Some(timeout / 3));
    }

    transport_config.max_idle_timeout(Some(timeout.try_into().unwrap()));

    // TODO: MTU discovery

    Arc::new(transport_config)
}

/// TODO: builder pattern
pub fn build_client_endpoint(
    ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
    congestion_mode: CongestionMode,
    keep_alive: bool,
) -> anyhow::Result<Endpoint> {
    let (tls_config, root_ca) = tls::build_client_config(ca, cert, key)?;

    let mut client_config = ClientConfig::new(Arc::new(tls_config));

    let transport_config = build_transport_config(keep_alive, congestion_mode);

    client_config.transport_config(transport_config);

    trace!(?client_config);

    // TODO: do we need to be careful about ipv4 vs ipv6 here?
    // TODO: io_uring
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;

    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

/// TODO: builder pattern
pub fn build_server_endpoint(
    ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
    stateless_retry: bool,
    listen: SocketAddr,
    congestion_mode: CongestionMode,
    keep_alive: bool,
) -> anyhow::Result<Endpoint> {
    let (tls_config, _root_ca) = tls::build_server_config(ca, cert, key)?;

    let mut server_config = ServerConfig::with_crypto(Arc::new(tls_config));

    let transport_config = build_transport_config(keep_alive, congestion_mode);

    server_config.transport_config(transport_config);

    // Introduces an additional round-trip to the handshake to make denial of service attacks more difficult.
    server_config.use_retry(stateless_retry);

    // TODO: no uni streams
    // TODO: lots more bi streams

    trace!(?server_config);

    // TODO: io_uring
    let endpoint = Endpoint::server(server_config, listen)?;

    Ok(endpoint)
}
