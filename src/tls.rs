// TODO: compare with <https://github.com/quinn-rs/quinn/blob/main/quinn/examples/common/mod.rs>

use anyhow::Context;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore, ServerConfig};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub fn build_root_store(root_ca: PathBuf) -> anyhow::Result<RootCertStore> {
    let mut root_store = RootCertStore::empty();

    let root_ca = File::open(root_ca).context("cannot open CA file")?;
    let mut root_ca = BufReader::new(root_ca);

    let ca_certs = rustls_pemfile::certs(&mut root_ca)?;
    root_store.add_parsable_certificates(&ca_certs);

    Ok(root_store)
}

pub fn build_client_config(
    root_ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
) -> anyhow::Result<ClientConfig> {
    let root_store = build_root_store(root_ca)?;

    let client_cert = File::open(cert).context("cannot open client cert file")?;
    let mut client_cert = BufReader::new(client_cert);

    let client_chain = rustls_pemfile::certs(&mut client_cert)?
        .into_iter()
        .map(Certificate)
        .collect();

    let client_key = File::open(key).context("cannot open client key file")?;
    let mut client_key = BufReader::new(client_key);

    // TODO: i don't like this. make a helper function
    let client_key = rustls_pemfile::pkcs8_private_keys(&mut client_key)?.remove(0);
    let client_key = PrivateKey(client_key);

    // TODO: do we configure client auth here?
    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_chain, client_key)?;

    Ok(config)
}

pub fn build_server_config(
    root_ca: PathBuf,
    cert: PathBuf,
    key: PathBuf,
) -> anyhow::Result<ServerConfig> {
    let root_store = build_root_store(root_ca)?;

    let server_cert = File::open(cert).context("cannot open server cert file")?;
    let mut server_cert = BufReader::new(server_cert);

    let server_chain = rustls_pemfile::certs(&mut server_cert)?
        .into_iter()
        .map(Certificate)
        .collect();

    let server_key = File::open(key).context("cannot open server key file")?;
    let mut server_key = BufReader::new(server_key);

    // TODO: i don't like this. make a helper function
    let server_key = rustls_pemfile::pkcs8_private_keys(&mut server_key)?.remove(0);
    let server_key = PrivateKey(server_key);

    let client_cert_verifier = AllowAnyAuthenticatedClient::new(root_store).boxed();

    // TODO: do we configure client auth here?
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(client_cert_verifier)
        .with_single_cert(server_chain, server_key)?;

    Ok(config)
}
