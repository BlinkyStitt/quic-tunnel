use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};

fn create_ca_certificate() -> Certificate {
    let mut params = CertificateParams::new(vec!["QUIC Tunnel Automatic".to_owned()]);

    // TODO: EC

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "QUIC Tunnel Automatic CA");

    params.distinguished_name = dn;

    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

    Certificate::from_params(params).expect("Failed to create CA certificate")
}

fn create_server_certificate(ca_cert: &Certificate, server_name: String) -> Certificate {
    let mut params = CertificateParams::new(vec![server_name]);
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;

    let server_cert =
        Certificate::from_params(params).expect("Failed to create server certificate");

    // Sign the server certificate with the CA
    let ca_key = &ca_cert.serialize_private_key_der();
    let ca_cert = &ca_cert.serialize_der().unwrap();
    let ca_key_pair = rcgen::KeyPair::from_der(&ca_key).unwrap();
    let ca_cert = rcgen::Certificate::from_der(&ca_cert, ca_key_pair).unwrap();
    server_cert.serialize_der_with_signer(&ca_cert).unwrap()
}

fn create_client_certificate(
    ca_cert: &Certificate,
    client_name: String,
) -> Result<Certificate, RcgenError> {
    let mut params = CertificateParams::new(vec![client_name]);

    // Set the client certificate's subject name
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "My Client Cert");
    params.distinguished_name = dn;

    // Specify that this certificate is not a CA
    params.is_ca = rcgen::IsCa::SelfSignedOnly;

    // Optional: Add extensions for client usage, like TLS Web Client Authentication
    // params.extended_key_usages.push(rcgen::ExtendedKeyUsagePurpose::ClientAuth);

    // Generate the key pair
    let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    params.key_pair = Some(key_pair);

    // Create the client certificate
    let client_cert = Certificate::from_params(params)?;

    // Sign the client certificate with the CA
    let ca_key = &ca_cert.serialize_private_key_der();
    let ca_cert_der = &ca_cert.serialize_der()?;
    let ca_key_pair = KeyPair::from_der(ca_key)?;
    let ca_cert = Certificate::from_der(ca_cert_der, ca_key_pair)?;

    // Serialize and sign the client certificate with the CA
    let client_cert_der = client_cert.serialize_der_with_signer(&ca_cert)?;

    // Deserialize to get the final certificate
    Ok(Certificate::from_der(
        &client_cert_der,
        client_cert.get_key_pair().clone(),
    )?)
}
