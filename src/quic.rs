use super::tls;
use quinn::{congestion, ClientConfig, Endpoint, ServerConfig};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use strum::EnumString;

#[derive(Default, EnumString)]
#[strum(ascii_case_insensitive)]
pub enum CongestionMode {
    /// good for high bandwidth networks
    Brr,
    /// good all around
    #[default]
    Cubic,
    /// good for high loss networks
    NewReno,
}

/// TODO: builder pattern
pub fn build_client_endpoint(ca: PathBuf, cert: PathBuf, key: PathBuf) -> anyhow::Result<Endpoint> {
    let (tls_config, root_ca) = tls::build_client_config(ca, cert, key)?;

    let client_config = ClientConfig::with_root_certificates(root_ca);

    // TODO: do we need to be careful about ipv4 vs ipv6 here?
    let mut endpoint = quinn::Endpoint::client("[::]:0".parse().unwrap())?;
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
) -> anyhow::Result<Endpoint> {
    let (tls_config, _root_ca) = tls::build_server_config(ca, cert, key)?;

    let mut server_config = ServerConfig::with_crypto(Arc::new(tls_config));

    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();

    // uni streams are not needed
    transport_config.max_concurrent_uni_streams(0_u8.into());

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

    // Introduces an additional round-trip to the handshake to make denial of service attacks more difficult.
    server_config.use_retry(stateless_retry);

    let endpoint = Endpoint::server(server_config, listen)?;

    Ok(endpoint)
}
