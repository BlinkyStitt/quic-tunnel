/// TODO: this uses blocking IO! Use tokio instead!
use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::Context;
use rcgen::{Certificate, CertificateParams};
use tracing::info;

pub struct TunnelCertificate {
    pub cert: rustls::Certificate,
    pub key: Option<rustls::PrivateKey>,
}

impl TunnelCertificate {
    pub fn load_or_new(ca: &Certificate, cert: PathBuf, key: PathBuf) -> anyhow::Result<Self> {
        if cert.exists() && key.exists() {
            Self::load_with_key(cert, key)
        } else {
            let subject_name = cert
                .file_stem()
                .context("no server name detected")?
                .to_str()
                .context("server name is not valid utf8")?
                .to_string();

            Self::new(ca, cert, key, subject_name)
        }
    }

    /// Create a new certificate and key signed by a CA.
    pub fn new(
        ca: &Certificate,
        cert: PathBuf,
        key: PathBuf,
        subject_name: String,
    ) -> anyhow::Result<Self> {
        info!("creating new certificate at \"{}\"", cert.display());

        assert!(!cert.exists());
        assert!(!key.exists());

        let params = CertificateParams::new([subject_name]);

        // TODO: set expiration
        // TODO: limit server/client certs with extenstions

        let x = Certificate::from_params(params)?;

        std::fs::write(cert, x.serialize_pem_with_signer(ca)?)?;
        std::fs::write(key, x.serialize_private_key_pem())?;

        let cert_der = x.serialize_der_with_signer(ca)?;
        let key_der = x.serialize_private_key_der();

        Ok(Self {
            cert: rustls::Certificate(cert_der),
            key: Some(rustls::PrivateKey(key_der)),
        })
    }

    /// Load an existing cert.
    pub fn load(cert: PathBuf) -> anyhow::Result<Self> {
        let cert = cert_from_pem(cert)?;
        let key = None;

        Ok(Self { cert, key })
    }

    /// Load an existing cert and key.
    pub fn load_with_key(cert: PathBuf, key: PathBuf) -> anyhow::Result<Self> {
        info!("loading existing certificate from \"{}\"", cert.display());

        let cert = cert_from_pem(cert)?;
        let key = Some(key_from_pem(key)?);

        Ok(Self { cert, key })
    }
}

/// get the first cert from a PEM file.
fn cert_from_pem(path: PathBuf) -> anyhow::Result<rustls::Certificate> {
    let mut reader = BufReader::new(File::open(path)?);

    let der = rustls_pemfile::certs(&mut reader)
        .next()
        .context("no key found")??;

    let key = rustls::Certificate(der.as_ref().to_vec());

    Ok(key)
}

/// get the first key from a PEM file.
fn key_from_pem(path: PathBuf) -> anyhow::Result<rustls::PrivateKey> {
    let mut reader = BufReader::new(File::open(path)?);

    let der = rustls_pemfile::private_key(&mut reader)?.context("no key found")?;

    let key = rustls::PrivateKey(der.secret_der().to_vec());

    Ok(key)
}
