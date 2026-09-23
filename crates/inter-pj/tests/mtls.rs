//! Real mutual-TLS handshakes against a local server that *requires* a client
//! certificate issued by a test CA. Every certificate is generated on the fly.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use inter_pj::{ClientIdentity, Credentials, Error, InterClient};
use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

struct Ca {
    issuer: CertifiedIssuer<'static, KeyPair>,
}

impl Ca {
    fn new(name: &str) -> Self {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name.push(DnType::CommonName, name);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let issuer = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
        Self { issuer }
    }

    fn pem(&self) -> String {
        self.issuer.pem()
    }

    fn der(&self) -> CertificateDer<'static> {
        self.issuer.der().clone()
    }

    /// Issues a leaf certificate; returns (certificate PEM, certificate DER, key).
    fn issue(
        &self,
        names: &[&str],
        usage: ExtendedKeyUsagePurpose,
    ) -> (String, CertificateDer<'static>, KeyPair) {
        let key = KeyPair::generate().unwrap();
        let mut params =
            CertificateParams::new(names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>())
                .unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, "folha de teste");
        params.extended_key_usages = vec![usage];
        let cert = params.signed_by(&key, &self.issuer).unwrap();
        (cert.pem(), cert.der().clone(), key)
    }
}

/// Counters kept by the test server.
#[derive(Default)]
struct Counters {
    /// TCP connections accepted.
    connections: AtomicUsize,
    /// Requests served over a handshake that presented a client certificate.
    served: AtomicUsize,
}

/// HTTPS server requiring client certificates signed by `client_ca`.
async fn start_server(server_ca: &Ca, client_ca: &Ca) -> (u16, Arc<Counters>) {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    roots.add(client_ca.der()).unwrap();
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .unwrap();

    let (_, server_der, server_key) =
        server_ca.issue(&["localhost"], ExtendedKeyUsagePurpose::ServerAuth);
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_client_cert_verifier(verifier)
        .with_single_cert(vec![server_der], key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let counters = Arc::new(Counters::default());
    let counter = counters.clone();

    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            counter.connections.fetch_add(1, Ordering::SeqCst);
            let acceptor = acceptor.clone();
            let counter = counter.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                if tls
                    .get_ref()
                    .1
                    .peer_certificates()
                    .is_some_and(|c| !c.is_empty())
                {
                    counter.served.fetch_add(1, Ordering::SeqCst);
                }
                let request = read_request(&mut tls).await;
                let body = if request.starts_with("POST /oauth/v2/token") {
                    r#"{"access_token":"tok-mtls","token_type":"Bearer","expires_in":3600,"scope":"extrato.read"}"#
                } else {
                    r#"{"disponivel":42.1}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    (port, counters)
}

/// Reads headers and body (by `content-length`) of one HTTP/1.1 request.
async fn read_request<S: AsyncReadExt + Unpin>(stream: &mut S) -> String {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n) = stream.read(&mut buf).await {
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data);
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

fn client(port: u16, trusted_server_ca: &Ca, identity: ClientIdentity) -> InterClient {
    InterClient::builder()
        .base_url(format!("https://localhost:{port}"))
        .credentials(Credentials::new("id", "segredo"))
        .identity(identity)
        .root_certificates_pem(trusted_server_ca.pem())
        .build()
        .unwrap()
}

#[tokio::test]
async fn completes_mutual_tls_with_trusted_client_certificate() {
    let ca = Ca::new("CA de teste");
    let (port, counters) = start_server(&ca, &ca).await;

    let (cert_pem, _, key) = ca.issue(&["cliente.teste"], ExtendedKeyUsagePurpose::ClientAuth);
    let identity =
        ClientIdentity::from_pem(cert_pem.as_bytes(), key.serialize_pem().as_bytes()).unwrap();

    let saldo = client(port, &ca, identity)
        .banking()
        .saldo(None)
        .await
        .unwrap();

    assert_eq!(saldo.disponivel.unwrap().to_string(), "42.1");
    assert_eq!(
        counters.served.load(Ordering::SeqCst),
        2,
        "token e saldo via mTLS"
    );
}

#[tokio::test]
async fn server_rejects_client_certificate_from_unknown_ca() {
    let ca = Ca::new("CA de teste");
    let (port, counters) = start_server(&ca, &ca).await;

    let other = Ca::new("CA desconhecida");
    let (cert_pem, _, key) = other.issue(&["intruso.teste"], ExtendedKeyUsagePurpose::ClientAuth);
    let identity =
        ClientIdentity::from_pem(cert_pem.as_bytes(), key.serialize_pem().as_bytes()).unwrap();

    let err = client(port, &ca, identity)
        .banking()
        .saldo(None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Transport(_)), "{err:?}");
    assert_eq!(counters.served.load(Ordering::SeqCst), 0);
    assert_eq!(
        counters.connections.load(Ordering::SeqCst),
        1,
        "falhas de TLS não são repetidas"
    );
}

#[tokio::test]
async fn client_rejects_server_certificate_from_untrusted_ca() {
    let server_ca = Ca::new("CA do servidor");
    let (port, counters) = start_server(&server_ca, &server_ca).await;

    let (cert_pem, _, key) =
        server_ca.issue(&["cliente.teste"], ExtendedKeyUsagePurpose::ClientAuth);
    let identity =
        ClientIdentity::from_pem(cert_pem.as_bytes(), key.serialize_pem().as_bytes()).unwrap();
    let unrelated = Ca::new("CA não relacionada");

    let err = client(port, &unrelated, identity)
        .banking()
        .saldo(None)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Transport(_)), "{err:?}");
    assert_eq!(counters.served.load(Ordering::SeqCst), 0);
    assert_eq!(
        counters.connections.load(Ordering::SeqCst),
        1,
        "falhas de TLS não são repetidas"
    );
}
