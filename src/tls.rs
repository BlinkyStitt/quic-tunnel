// TODO: compare with <https://github.com/quinn-rs/quinn/blob/main/quinn/examples/common/mod.rs>

use anyhow::Context;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore, ServerConfig};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub fn build_root_store(root_certs: &[&Certificate]) -> anyhow::Result<RootCertStore> {
    let mut root_store = RootCertStore::empty();

    for root_cert in root_certs {
        root_store.add(root_cert)?;
    }

    Ok(root_store)
}

fn read_cert_chain(cert_chain: PathBuf) -> anyhow::Result<Vec<Certificate>> {
    let cert_reader: File = File::open(cert_chain).context("cannot open client cert file")?;

    let mut cert_reader = BufReader::new(cert_reader);

    let cert_chain: Vec<_> = rustls_pemfile::certs(&mut cert_reader)?
        .into_iter()
        .map(Certificate)
        .collect();

    assert!(
        cert_chain.len() >= 2,
        "client cert chain must have at least 2 certs"
    );

    Ok(cert_chain)
}

pub fn build_client_config(
    cert_chain: PathBuf,
    key: PathBuf,
) -> anyhow::Result<(ClientConfig, RootCertStore)> {
    let cert_chain = read_cert_chain(cert_chain)?;

    let key_reader = File::open(key).context("cannot open client key file")?;
    let mut key_reader = BufReader::new(key_reader);

    let root_cert = cert_chain.last().unwrap();
    let root_store = build_root_store(&[root_cert])?;

    // TODO: i don't like this. make a helper function?
    let mut client_key = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?;
    assert_eq!(client_key.len(), 1);
    let client_key = PrivateKey(client_key.remove(0));

    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store.clone())
        .with_client_auth_cert(cert_chain, client_key)?;

    Ok((config, root_store))
}

pub fn build_server_config(
    cert_chain: PathBuf,
    key: PathBuf,
) -> anyhow::Result<(ServerConfig, RootCertStore)> {
    let cert_chain = read_cert_chain(cert_chain)?;

    let key_reader = File::open(key).context("cannot open client key file")?;
    let mut key_reader = BufReader::new(key_reader);

    let mut key = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?;
    assert_eq!(key.len(), 1);
    let key_der = PrivateKey(key.remove(0));

    let root_cert = cert_chain.last().unwrap();
    let root_store = build_root_store(&[root_cert])?;

    let client_cert_verifier = AllowAnyAuthenticatedClient::new(root_store.clone()).boxed();

    // TODO: do we configure client auth here?
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(cert_chain, key_der)?;

    Ok((config, root_store))
}
