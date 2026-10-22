use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use quic_tunnel::{
    certs::{CertificateAuthority, TunnelCertificate, TunnelEnd},
    quic::{accept_connection, build_client_endpoint, build_server_endpoint, CongestionMode},
};

struct Identity {
    cert: PathBuf,
    key: PathBuf,
}

fn issue(
    ca: &CertificateAuthority,
    dir: &Path,
    name: &str,
    end: TunnelEnd,
) -> anyhow::Result<Identity> {
    let identity = Identity {
        cert: dir.join(format!("{name}.pem")),
        key: dir.join(format!("{name}.key.pem")),
    };
    TunnelCertificate::new(
        ca,
        identity.cert.clone(),
        identity.key.clone(),
        "localhost".into(),
        end,
    )?;
    Ok(identity)
}

#[tokio::test]
async fn trusted_client_exchanges_data_after_retry() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let ca_path = dir.path().join("ca.pem");
    let ca_key = dir.path().join("ca.key.pem");
    let ca = CertificateAuthority::new(ca_path.clone(), ca_key.clone())?;
    let server_id = issue(&ca, dir.path(), "server", TunnelEnd::Server)?;
    let loaded_ca = CertificateAuthority::load(ca_path.clone(), ca_key)?;
    let client_id = issue(&loaded_ca, dir.path(), "client", TunnelEnd::Client)?;
    let server = build_server_endpoint(
        ca_path.clone(),
        server_id.cert,
        server_id.key,
        "127.0.0.1:0".parse()?,
        CongestionMode::default(),
        false,
    )?;
    let client = build_client_endpoint(
        ca_path,
        client_id.cert,
        client_id.key,
        CongestionMode::default(),
        false,
    )?;
    let address = server.local_addr()?;
    let server_handshake = async {
        accept_connection(&server)
            .await
            .expect("incoming connection")
            .await
    };
    let (client_connection, server_connection) =
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                client.connect(address, "localhost").unwrap(),
                server_handshake
            )
        })
        .await?;
    let client_connection = client_connection?;
    let server_connection = server_connection?;

    let receive = async {
        let (mut send, mut recv) = server_connection.accept_bi().await?;
        assert_eq!(recv.read_to_end(16).await?, b"ping");
        send.write_all(b"pong").await?;
        send.finish()?;
        Ok::<_, anyhow::Error>(())
    };
    let send = async {
        let (mut send, mut recv) = client_connection.open_bi().await?;
        send.write_all(b"ping").await?;
        send.finish()?;
        assert_eq!(recv.read_to_end(16).await?, b"pong");
        Ok::<_, anyhow::Error>(())
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(receive, send)
    })
    .await??;
    client.close(0u32.into(), b"test complete");
    server.close(0u32.into(), b"test complete");
    Ok(())
}

#[tokio::test]
async fn server_rejects_client_signed_by_another_ca() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let ca_path = dir.path().join("ca.pem");
    let ca = CertificateAuthority::new(ca_path.clone(), dir.path().join("ca.key.pem"))?;
    let other_ca = CertificateAuthority::new(
        dir.path().join("other-ca.pem"),
        dir.path().join("other-ca.key.pem"),
    )?;
    let server_id = issue(&ca, dir.path(), "server", TunnelEnd::Server)?;
    let client_id = issue(&other_ca, dir.path(), "untrusted-client", TunnelEnd::Client)?;
    let server = build_server_endpoint(
        ca_path.clone(),
        server_id.cert,
        server_id.key,
        "127.0.0.1:0".parse()?,
        CongestionMode::default(),
        false,
    )?;
    let client = build_client_endpoint(
        ca_path,
        client_id.cert,
        client_id.key,
        CongestionMode::default(),
        false,
    )?;
    let address = server.local_addr()?;
    let server_handshake = async {
        accept_connection(&server)
            .await
            .expect("incoming connection")
            .await
    };
    let (_, server_result) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            client.connect(address, "localhost").unwrap(),
            server_handshake
        )
    })
    .await?;
    assert!(
        matches!(
            server_result,
            Err(quinn::ConnectionError::TransportError(_))
        ),
        "server must reject the untrusted certificate: {server_result:?}"
    );
    client.close(0u32.into(), b"test complete");
    server.close(0u32.into(), b"test complete");
    Ok(())
}
