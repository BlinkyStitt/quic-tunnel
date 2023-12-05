/// TODO: this uses blocking IO! Use tokio instead!
use std::path::PathBuf;

use rcgen::{Certificate, CertificateParams, KeyPair, RcgenError};
use tracing::info;

use crate::certs::DEFAULT_ALG;

pub struct CertificateAuthority {
    pub cert_gen: Certificate,
}

impl CertificateAuthority {
    /// TODO: automated renewal if expiring soon
    pub fn load_or_new(cert: PathBuf, key: PathBuf) -> anyhow::Result<Self> {
        if cert.exists() && key.exists() {
            Self::load(cert, key)
        } else {
            Self::new(cert, key)
        }
    }

    /// Create a new CA.
    ///
    /// If you want a different name or whatever, you should probably use real certificate management software instead of this.
    pub fn new(cert: PathBuf, key: PathBuf) -> anyhow::Result<Self> {
        info!("creating new CA cert at {}", cert.display());

        assert!(!cert.exists());
        assert!(!key.exists());

        let mut ca_params = CertificateParams::new([]);

        // TODO: set expiration

        ca_params.alg = DEFAULT_ALG;
        ca_params
            .distinguished_name
            .push(rcgen::DnType::OrganizationName, "QUIC Tunnel");
        ca_params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "QUIC Tunnel Automatic CA");
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            rcgen::KeyUsagePurpose::KeyCertSign,
            rcgen::KeyUsagePurpose::DigitalSignature,
            rcgen::KeyUsagePurpose::CrlSign,
        ];

        let x = Certificate::from_params(ca_params)?;

        std::fs::write(cert, x.serialize_pem()?)?;
        std::fs::write(key, x.serialize_private_key_pem())?;

        Ok(Self { cert_gen: x })
    }

    /// Load an existing CA.
    pub fn load(cert: PathBuf, key: PathBuf) -> anyhow::Result<Self> {
        info!("loading existing CA cert from {}", cert.display());

        let pem_str = std::fs::read_to_string(cert)?;

        let key_str = std::fs::read_to_string(key)?;
        let key_pair = KeyPair::from_pem(&key_str)?;

        let params = CertificateParams::from_ca_cert_pem(&pem_str, key_pair)?;

        let x = Certificate::from_params(params)?;

        Ok(Self { cert_gen: x })
    }

    pub fn cert(&self) -> Result<rustls::Certificate, RcgenError> {
        let der = self.cert_gen.serialize_der()?;

        let cert = rustls::Certificate(der);

        Ok(cert)
    }

    pub fn key(&self) -> rustls::PrivateKey {
        let der = self.cert_gen.serialize_private_key_der();

        rustls::PrivateKey(der)
    }
}
