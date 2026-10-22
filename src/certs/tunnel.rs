/// TODO: this uses blocking IO! Use tokio instead!
use std::path::PathBuf;

use anyhow::Context;
use rcgen::{CertificateParams, KeyPair};
use rustls::pki_types::pem::PemObject;
use strum::EnumString;
use tracing::info;

use crate::certs::{CertificateAuthority, DEFAULT_ALG};

pub struct TunnelCertificate {
    pub cert: rustls::pki_types::CertificateDer<'static>,
    pub key: Option<rustls::pki_types::PrivateKeyDer<'static>>,
}

#[derive(Copy, Clone, Debug, EnumString)]
pub enum TunnelEnd {
    Server,
    Client,
}

impl TunnelCertificate {
    /// TODO: automated renewal if expiring soon
    pub fn load_or_new(
        ca: &CertificateAuthority,
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
        ca: &CertificateAuthority,
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
                let mut params = CertificateParams::new([])?;

                params
                    .distinguished_name
                    .push(rcgen::DnType::CommonName, subject_name);

                params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];

                // TODO: what serial? 0xc0ffee just a placeholder
                params.serial_number = Some(rcgen::SerialNumber::from(vec![0xC0, 0xFF, 0xEE]));

                params
            }
            TunnelEnd::Server => {
                let mut params = CertificateParams::new([subject_name])?;

                params
                    .distinguished_name
                    .push(rcgen::DnType::CommonName, "Example Client");

                params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];

                params
            }
        };

        params.is_ca = rcgen::IsCa::NoCa;

        let key_pair = KeyPair::generate_for(DEFAULT_ALG)?;
        let x = params.signed_by(&key_pair, &ca.cert_gen, &ca.key_pair)?;

        std::fs::write(cert, x.pem())?;
        std::fs::write(key, key_pair.serialize_pem())?;

        let cert_der = x.der().clone();
        let key_der = key_pair.serialize_der();

        Ok(Self {
            cert: cert_der,
            key: Some(rustls::pki_types::PrivatePkcs8KeyDer::from(key_der).into()),
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
pub fn cert_from_pem(path: PathBuf) -> anyhow::Result<rustls::pki_types::CertificateDer<'static>> {
    info!("loading certificate from \"{}\"", path.display());

    rustls::pki_types::CertificateDer::from_pem_file(&path).with_context(|| {
        format!(
            "failed reading {}. maybe run the 'certs' command?",
            path.display()
        )
    })
}

/// get the first key from a PEM file.
pub fn key_from_pem(path: PathBuf) -> anyhow::Result<rustls::pki_types::PrivateKeyDer<'static>> {
    info!("loading key from \"{}\"", path.display());

    rustls::pki_types::PrivateKeyDer::from_pem_file(&path)
        .with_context(|| format!("failed reading key {}", path.display()))
}
