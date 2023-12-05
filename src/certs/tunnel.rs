/// TODO: this uses blocking IO! Use tokio instead!
use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::Context;
use rcgen::{Certificate, CertificateParams};
use strum::EnumString;
use tracing::info;

use crate::certs::DEFAULT_ALG;

pub struct TunnelCertificate {
    pub cert: rustls::Certificate,
    pub key: Option<rustls::PrivateKey>,
}

#[derive(Copy, Clone, Debug, EnumString)]
pub enum TunnelEnd {
    Server,
    Client,
}

impl TunnelCertificate {
    /// TODO: automated renewal if expiring soon
    pub fn load_or_new(
        ca: &Certificate,
        cert: PathBuf,
        key: PathBuf,
        tunnel_end: TunnelEnd,
    ) -> anyhow::Result<Self> {
        if cert.exists() && key.exists() {
            Self::load_with_key(cert, key)
        } else {
            let subject_name = cert
                .file_stem()
                .context("no server name detected")?
                .to_str()
                .context("server name is not valid utf8")?
                .to_string();

            Self::new(ca, cert, key, subject_name, tunnel_end)
        }
    }

    /// Create a new certificate and key signed by a CA.
    pub fn new(
        ca: &Certificate,
        cert: PathBuf,
        key: PathBuf,
        subject_name: String,
        tunnel_end: TunnelEnd,
    ) -> anyhow::Result<Self> {
        info!("creating new certificate at \"{}\"", cert.display());

        assert!(!cert.exists());
        assert!(!key.exists());

        // TODO: set expiration
        // TODO: limit server/client certs with extenstions

        let mut params = match tunnel_end {
            TunnelEnd::Client => {
                let mut params = CertificateParams::new([]);

                params
                    .distinguished_name
                    .push(rcgen::DnType::CommonName, subject_name);

                params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];

                // TODO: what serial? 0xc0ffee just a placeholder
                params.serial_number = Some(rcgen::SerialNumber::from(vec![0xC0, 0xFF, 0xEE]));

                params
            }
            TunnelEnd::Server => {
                let mut params = CertificateParams::new([subject_name]);

                params
                    .distinguished_name
                    .push(rcgen::DnType::CommonName, "Example Client");

                params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];

                params
            }
        };

        params.alg = DEFAULT_ALG;
        params.is_ca = rcgen::IsCa::NoCa;

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
pub fn cert_from_pem(path: PathBuf) -> anyhow::Result<rustls::Certificate> {
    info!("loading certificate from \"{}\"", path.display());

    let mut reader = BufReader::new(File::open(path.clone()).context(format!(
        "failed opening {}. maybe run the 'certs' command?",
        path.display()
    ))?);

    let der = rustls_pemfile::certs(&mut reader)
        .next()
        .context("no key found")??;

    let key = rustls::Certificate(der.as_ref().to_vec());

    Ok(key)
}

/// get the first key from a PEM file.
pub fn key_from_pem(path: PathBuf) -> anyhow::Result<rustls::PrivateKey> {
    info!("loading key from \"{}\"", path.display());

    let mut reader = BufReader::new(File::open(path)?);

    let der = rustls_pemfile::private_key(&mut reader)?.context("no key found")?;

    let key = rustls::PrivateKey(der.secret_der().to_vec());

    Ok(key)
}
