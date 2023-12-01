use super::tls;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

/// TODO: builder pattern
pub fn build_client_endpoint(cert_chain: PathBuf, key: PathBuf) -> anyhow::Result<Endpoint> {
    let (mut tls_config, root_ca) = tls::build_client_config(cert_chain, key)?;

    tls_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    let client_config = ClientConfig::with_root_certificates(root_ca);

    // TODO: do we need to be careful about ipv4 vs ipv6 here?
    let mut endpoint = quinn::Endpoint::client("[::]:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

/// TODO: builder pattern
pub fn build_server_endpoint(
    cert_chain: PathBuf,
    key: PathBuf,
    stateless_retry: bool,
    listen: SocketAddr,
) -> anyhow::Result<Endpoint> {
    let (mut tls_config, _root_ca) = tls::build_server_config(cert_chain, key)?;

    tls_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    let mut server_config = ServerConfig::with_crypto(Arc::new(tls_config));

    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();

    // uni streams are not needed
    transport_config.max_concurrent_uni_streams(0_u8.into());

    // TODO: what is this?
    if stateless_retry {
        server_config.use_retry(true);
    }

    let endpoint = Endpoint::server(server_config, listen)?;

    Ok(endpoint)
}
