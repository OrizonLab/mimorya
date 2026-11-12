//! Certificates for the tests' simulated servers that speak TLS (the mail
//! servers and a proxy, ADR-205): an authority made when a test starts,
//! whose root a client trusts through its network policy alone
//! (`test_roots`), and the certificate it signs for the server's names. A
//! certificate of another authority, or one naming another server, is what
//! a client must refuse. A server answering lines is given for the network
//! layer's own tests.

use std::net::SocketAddr;
use std::sync::Arc;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// The names a test server's certificate holds unless told otherwise.
pub const LOCAL_NAMES: [&str; 2] = ["localhost", "127.0.0.1"];

/// An authority of the tests and the certificate it signed for a server.
pub struct TestAuthority {
    root: CertificateDer<'static>,
    server: Arc<ServerConfig>,
}

impl TestAuthority {
    /// An authority whose server certificate names `localhost` and
    /// `127.0.0.1`.
    pub fn new() -> Self {
        Self::naming(&LOCAL_NAMES)
    }

    /// An authority whose server certificate names `names` alone (host
    /// names or literal addresses).
    pub fn naming(names: &[&str]) -> Self {
        let authority_key = KeyPair::generate().expect("authority key");
        let mut authority = CertificateParams::new(Vec::<String>::new()).expect("authority");
        authority.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        authority
            .distinguished_name
            .push(DnType::CommonName, "Autorité des tests Mimorya");
        authority.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let root = authority.self_signed(&authority_key).expect("root");
        let issuer = Issuer::new(authority, authority_key);
        let key = KeyPair::generate().expect("server key");
        let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        let mut params = CertificateParams::new(names.clone()).expect("server names");
        if let Some(first) = names.first() {
            params
                .distinguished_name
                .push(DnType::CommonName, first.as_str());
        }
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let certificate = params.signed_by(&key, &issuer).expect("server certificate");
        let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let server =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("protocol versions")
                .with_no_client_auth()
                .with_single_cert(vec![certificate.der().clone()], private)
                .expect("server configuration");
        Self {
            root: root.der().clone(),
            server: Arc::new(server),
        }
    }

    /// Its root, as a network policy trusts it: kept for the process, as
    /// the policy's other test values are.
    pub fn roots(&self) -> &'static [CertificateDer<'static>] {
        Vec::leak(vec![self.root.clone()])
    }

    /// What its servers accept connections with.
    pub fn acceptor(&self) -> TlsAcceptor {
        TlsAcceptor::from(self.server.clone())
    }
}

impl Default for TestAuthority {
    fn default() -> Self {
        Self::new()
    }
}

/// A server of `authority` on a local port: once TLS is set up, it says
/// `greeting`, then sends back each line it reads, `echo: ` before it.
pub async fn serve_tls_echo(authority: &TestAuthority, greeting: &str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("local port");
    let address = listener.local_addr().expect("local address");
    let (acceptor, greeting) = (authority.acceptor(), format!("{greeting}\r\n"));
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let (acceptor, greeting) = (acceptor.clone(), greeting.clone());
            tokio::spawn(async move {
                let Ok(stream) = acceptor.accept(socket).await else {
                    return;
                };
                let (reader, mut writer) = tokio::io::split(stream);
                if writer.write_all(greeting.as_bytes()).await.is_err() {
                    return;
                }
                let mut lines = BufReader::new(reader).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let answer = format!("echo: {line}\r\n");
                    if writer.write_all(answer.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    address
}
