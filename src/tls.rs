// TODO: compare with <https://github.com/quinn-rs/quinn/blob/main/quinn/examples/common/mod.rs>

use crate::certs::{cert_from_pem, key_from_pem};
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::{Certificate, ClientConfig, RootCertStore, ServerConfig};
use std::path::PathBuf;

pub fn build_root_store(root_certs: &[&Certificate]) -> anyhow::Result<RootCertStore> {
    let mut root_store = RootCertStore::empty();

    for root_cert in root_certs {
        root_store.add(root_cert)?;
    }

    Ok(root_store)
}

pub fn build_client_config(
    ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
) -> anyhow::Result<(ClientConfig, RootCertStore)> {
    let ca = cert_from_pem(ca)?;
    let cert = cert_from_pem(cert)?;
    let key = key_from_pem(key)?;

    let root_store = build_root_store(&[&ca])?;

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store.clone())
        // .with_no_client_auth();
        .with_client_auth_cert(vec![cert, ca], key)?;

    Ok((config, root_store))
}

pub fn build_server_config(
    ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
) -> anyhow::Result<(ServerConfig, RootCertStore)> {
    let ca = cert_from_pem(ca)?;
    let cert = cert_from_pem(cert)?;
    let key = key_from_pem(key)?;

    let root_store = build_root_store(&[&ca])?;

    // accept any client cert signed by the CA
    // TODO: figure out why certs aren't working
    // server says `DEBUG quinn_proto::connection: closing connection due to transport error: the cryptographic handshake failed: error 116: peer sent no certificates`
    // client says `DEBUG rustls::client::common: Client auth requested but no cert/sigscheme available`
    let client_cert_verifier = AllowAnyAuthenticatedClient::new(root_store.clone()).boxed();

    // TODO: do we configure client auth here?
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        // .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(vec![cert, ca], key)?;

    Ok((config, root_store))
}
