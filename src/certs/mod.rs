mod ca;
mod tunnel;

pub use ca::CertificateAuthority;
pub use tunnel::{cert_from_pem, key_from_pem, TunnelCertificate, TunnelEnd};

pub static DEFAULT_ALG: &rcgen::SignatureAlgorithm = &rcgen::PKCS_ECDSA_P256_SHA256;
