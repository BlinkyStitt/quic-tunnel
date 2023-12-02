use std::path::PathBuf;

use argh::FromArgs;
use quic_tunnel::{
    certs::{CertificateAuthority, TunnelCertificate},
    log::configure_logging,
};
use tracing::info;

#[derive(FromArgs)]
/// Generage certs easily.
/// There's a lot more this could do, but I feel like you should just use other existing Certificate Authority management software if need anything more than this
///
/// TODO: handle expiration, renewals, revocation, etc.
/// TODO: certificate signing requests. separate cert generation and CA signing steps
struct Certs {
    #[argh(positional)]
    /// the directory to write certs to
    dir: PathBuf,

    /// names of the client certs to generate (if they don't already exist)
    #[argh(positional, greedy)]
    client_names: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let command: Certs = argh::from_env();

    configure_logging();

    // get or create the certificate authority
    let ca_cert = command.dir.join("ca_cert.pem");
    let ca_key = command.dir.join("ca_key.pem");

    let ca = CertificateAuthority::load_or_new(ca_cert, ca_key)?;

    // get or create the server certificate
    let server_cert = command.dir.join("server_cert.pem");
    let server_key = command.dir.join("server_key.pem");

    TunnelCertificate::load_or_new(&ca.cert_gen, server_cert, server_key)?;

    // get or create all of the client certificates
    for client_name in command.client_names {
        let client_cert = command.dir.join(format!("{}_client.pem", client_name));
        let client_key = command.dir.join(format!("{}_client.key", client_name));

        TunnelCertificate::load_or_new(&ca.cert_gen, client_cert, client_key)?;
    }

    info!("saved certs to \"{}\"", command.dir.display());

    Ok(())
}
