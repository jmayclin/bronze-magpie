use std::sync::{Arc, Once, RwLock};

use axum::extract::path::ErrorKind::ParseErrorAtIndex;
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Error::Crypto, Identifier, NewAccount, NewOrder,
    RetryPolicy,
};
use rcgen::{CertificateParams, CustomExtension, KeyPair};
use rustls::{
    crypto::{self, CryptoProvider},
    lock::Mutex,
    pki_types::CertificateDer,
    server::ResolvesServerCert,
};

const DOMAIN: &str = "bronzemagpie.com";
const ACME_ALPN: &[u8] = b"tls-alpn-01";

struct CertContainer {
    private_key_pem: String,
    cert_chain_pem: String,
}

static INSTALL_CRYPTO_PROVIDER: Once = Once::new();

impl CertContainer {
    fn to_rustls(&self) -> rustls::sign::CertifiedKey {
        let mut chain_reader = std::io::Cursor::new(self.cert_chain_pem.as_bytes());
        let chain_der: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut chain_reader)
            .map(|cert| cert.unwrap())
            .collect();

        let mut key_reader = std::io::Cursor::new(self.private_key_pem.as_bytes());
        let key_der = rustls_pemfile::private_key(&mut key_reader)
            .unwrap()
            .unwrap();
        rustls::sign::CertifiedKey::from_der(
            chain_der,
            key_der,
            CryptoProvider::get_default().unwrap(),
        )
        .unwrap()
    }
}

async fn order_cert() -> anyhow::Result<CertContainer> {
    // let (provider, rustls_crypto_provider) = (
    //     instant_acme::CryptoProvider::aws_lc_rs(),
    //     rustls::crypto::aws_lc_rs::default_provider(),
    // );

    let account = NewAccount {
        contact: &["jam.mayc@gmail.com"],
        terms_of_service_agreed: true,
        only_return_existing: true,
    };
    tracing::info!("account created: {account:?}");

    let builder = Account::builder().unwrap();
    // TODO: cache account credentials
    let (account, credentials) = builder.create(&account, "idk".to_string(), None).await?;

    let mut order = account
        .new_order(&NewOrder::new(&[Identifier::Dns(DOMAIN.to_string())]))
        .await?;

    let mut authorizations = order.authorizations();
    let mut authorization = authorizations.next().await.unwrap().unwrap();
    assert_eq!(authorization.status, AuthorizationStatus::Pending);

    let mut tls_challenge = authorization.challenge(ChallengeType::TlsAlpn01).unwrap();
    tracing::debug!("got tls challenge: {:?}", *tls_challenge);
    let key_authorization = tls_challenge.key_authorization();

    // tell the ACME server that we are ready for validation
    tls_challenge.set_ready().await?;

    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .expect("order failed to complete");

    let private_key_pem = order
        .finalize()
        .await
        .expect("failed to generate private key");
    let cert_chain_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .expect("failed to get signed cert");

    Ok(CertContainer {
        private_key_pem,
        cert_chain_pem,
    })
}

/// The client prepares for validation by constructing a self-signed certificate
/// that MUST contain an acmeIdentifier extension and a subjectAlternativeName
/// extension [RFC5280]. The subjectAlternativeName extension MUST contain a single
/// dNSName entry where the value is the domain name being validated. The
/// acmeIdentifier extension MUST contain the SHA-256 digest [FIPS180-4] of the key
///  authorization [RFC8555] for the challenge. The acmeIdentifier extension MUST
/// be critical so that the certificate isn't inadvertently used by non-ACME software.
fn challenge_certificate(authorization_digest: &[u8]) -> CertContainer {
    let mut params = CertificateParams::new(vec![DOMAIN.to_string()]).unwrap();
    let acme_extension = CustomExtension::new_acme_identifier(authorization_digest);
    params.custom_extensions.push(acme_extension);

    let key = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    CertContainer {
        private_key_pem: key.serialize_pem(),
        cert_chain_pem: cert.pem(),
    }
}

/// Handles cert resolution
/// - prefer to get the cached valid cert from lets encrypt, otherwise create a self signed cert
/// - handles resolution for the ALPN challenge stuff
#[derive(Debug)]
pub(crate) struct CertResolver {
    /// starts as a self signed cert, until the cert is obtained
    /// from lets-encrypt
    pub default_cert: RwLock<Arc<rustls::sign::CertifiedKey>>,

    pub acme_challenge: Mutex<Option<Arc<rustls::sign::CertifiedKey>>>,
}

impl CertResolver {
    const CERT_DIRECTORY: &'static str = "/etc/bronze-magpie/";
    const CHAIN_NAME: &'static str = "chain.pem";
    const KEY_NAME: &'static str = "key.pem";
    pub fn new() -> Self {
        INSTALL_CRYPTO_PROVIDER.call_once(|| {
            crypto::aws_lc_rs::default_provider()
                .install_default()
                .unwrap()
        });

        let default_cert = if let Some(cert) = Self::from_cache() {
            tracing::info!("no cert found in cache");
            cert
        } else {
            Self::self_signed()
        };

        let default_cert = default_cert.to_rustls();
        Self {
            default_cert: RwLock::new(Arc::new(default_cert)),
            acme_challenge: Mutex::new(None),
        }
    }

    fn self_signed() -> CertContainer {
        let cert = rcgen::generate_simple_self_signed(vec![DOMAIN.to_string()]).unwrap();
        let private_key_pem = cert.signing_key.serialize_pem();
        let cert_chain_pem = cert.cert.pem();
        CertContainer {
            private_key_pem,
            cert_chain_pem,
        }
    }

    fn from_cache() -> Option<CertContainer> {
        let chain = {
            let chain_path = {
                let mut path = String::new();
                path.push_str(Self::CERT_DIRECTORY);
                path.push_str(Self::CHAIN_NAME);
                path
            };

            std::fs::read_to_string(chain_path).ok()
        };

        let key = {
            let path = {
                let mut path = String::new();
                path.push_str(Self::CERT_DIRECTORY);
                path.push_str(Self::KEY_NAME);
                path
            };

            std::fs::read_to_string(path).ok()
        };
        if let (Some(private_key_pem), Some(cert_chain_pem)) = (key, chain) {
            Some(CertContainer {
                private_key_pem,
                cert_chain_pem,
            })
        } else {
            None
        }
    }
}

impl ResolvesServerCert for CertResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        let acme_alpn = client_hello
            .alpn()
            .map(|mut alpns| alpns.any(|alpn| alpn == ACME_ALPN))
            .unwrap_or(false);
        if acme_alpn {
            tracing::info!("serving the acme challenge cert");
            self.acme_challenge.lock().map_or(None, |cert| cert.clone())
        } else {
            Some(self.default_cert.read().unwrap().clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// make sure that we can create a cert without any of the crypto provider things panicking
    #[test]
    fn cert_creation() {
        let fake_digest = [0; 32];
        let cert = challenge_certificate(&fake_digest);

        let server_config = rustls::ServerConfig::builder();
        //server_config.with_no_client_auth().with_cert_resolver(cert_resolver)
    }

    #[test]
    fn acme_serving() {
        let fake_digest = [0; 32];
        let cert = challenge_certificate(&fake_digest);

        let cert_resolver = Arc::new(CertResolver::new());

        let server_config = rustls::ServerConfig::builder();

        let config = server_config
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver);
    }
}
