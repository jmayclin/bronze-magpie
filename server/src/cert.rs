use std::{
    sync::{Arc, Once, RwLock},
    time::Duration,
};

use axum::extract::path::ErrorKind::ParseErrorAtIndex;
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Error::Crypto, Identifier, NewAccount, NewOrder,
    OrderStatus, RetryPolicy,
};
use rcgen::{CertificateParams, CustomExtension, KeyPair};
use rustls::{
    crypto::CryptoProvider, lock::Mutex, pki_types::CertificateDer, server::ResolvesServerCert,
};
use time::OffsetDateTime;

const DOMAIN: &str = "bronzemagpie.com";
const ACME_ALPN: &[u8] = b"tls-alpn-01";
const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";
struct CertContainer {
    private_key_pem: String,
    cert_chain_pem: String,
}

static INSTALL_CRYPTO_PROVIDER: Once = Once::new();

impl CertContainer {
    const CERT_DIRECTORY: &'static str = "/etc/bronze-magpie/";
    const CHAIN_NAME: &'static str = "chain.pem";
    const KEY_NAME: &'static str = "key.pem";

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

    fn public_cert_path() -> String {
        let mut path = String::new();
        path.push_str(Self::CERT_DIRECTORY);
        path.push_str(Self::CHAIN_NAME);
        path
    }

    fn default_key_path() -> String {
        let mut path = String::new();
        path.push_str(Self::CERT_DIRECTORY);
        path.push_str(Self::KEY_NAME);
        path
    }

    fn from_cache() -> Option<CertContainer> {
        let chain = std::fs::read_to_string(Self::public_cert_path()).ok();
        let key = std::fs::read_to_string(Self::default_key_path()).ok();

        if let (Some(private_key_pem), Some(cert_chain_pem)) = (key, chain) {
            Some(CertContainer {
                private_key_pem,
                cert_chain_pem,
            })
        } else {
            None
        }
    }

    fn cache(&self) {
        std::fs::write(Self::public_cert_path(), self.cert_chain_pem.as_bytes()).unwrap();
        std::fs::write(Self::default_key_path(), self.private_key_pem.as_bytes()).unwrap();
    }
}

struct CertOrder {
    order: instant_acme::Order,
}

impl CertOrder {
    // initiate the ACME flow
    pub async fn create() -> anyhow::Result<Self> {
        let account = NewAccount {
            contact: &["jam.mayc@gmail.com"],
            terms_of_service_agreed: true,
            only_return_existing: true,
        };

        let builder = Account::default_builder().unwrap();
        // TODO: cache account credentials
        let (account, credentials) = builder.create(&account, "idk".to_string(), None).await?;

        let order = account
            .new_order(&NewOrder::new(&[Identifier::Dns(DOMAIN.to_string())]))
            .await?;
        Ok(Self { order })
    }

    // get the appropriate cert to serve in the TLS challenge
    pub async fn acme_cert(&mut self) -> anyhow::Result<CertContainer> {
        let mut authorizations = self.order.authorizations();
        let mut authorization = authorizations.next().await.unwrap()?;
        let tls_challenge = authorization.challenge(ChallengeType::TlsAlpn01).unwrap();
        let thumbprint_digest = tls_challenge.key_authorization().unwrap();
        Ok(challenge_certificate(thumbprint_digest.digest().as_ref()))
    }

    // finish the order after the cert has been configured on the endpoint
    pub async fn finish(mut self) -> anyhow::Result<CertContainer> {
        let mut authorizations = self.order.authorizations();
        let mut authorization = authorizations.next().await.unwrap()?;
        let mut tls_challenge = authorization.challenge(ChallengeType::TlsAlpn01).unwrap();
        tls_challenge.set_ready().await?;

        let status = self.order.poll_ready(&RetryPolicy::default()).await?;
        assert_eq!(status, OrderStatus::Ready);

        let private_key_pem = self.order.finalize().await?;
        let cert_chain_pem = self.order.poll_certificate(&RetryPolicy::default()).await?;

        Ok(CertContainer {
            private_key_pem,
            cert_chain_pem,
        })
    }
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
    pub default_cert: Arc<RwLock<Arc<rustls::sign::CertifiedKey>>>,

    // TODO: make this less ugly
    // Arc 1 -> because the updated needs access
    // Arc 2 -> because rustls wants it to be reference counted
    pub acme_challenge: Arc<Mutex<Option<Arc<rustls::sign::CertifiedKey>>>>,
}

impl CertResolver {
    const CERT_DIRECTORY: &'static str = "/etc/bronze-magpie/";
    const CHAIN_NAME: &'static str = "chain.pem";
    const KEY_NAME: &'static str = "key.pem";

    pub fn new() -> Self {
        INSTALL_CRYPTO_PROVIDER.call_once(|| {
            rustls_rustcrypto::provider().install_default().unwrap()
            // crypto::aws_lc_rs::default_provider()
            //     .install_default()
            //     .unwrap()
        });

        let default_cert = if let Some(cert) = CertContainer::from_cache() {
            tracing::info!("cert found in cache");
            cert
        } else {
            tracing::info!("no cert found in cache, creating self signed cert");
            Self::self_signed()
        };

        let default_cert = default_cert.to_rustls();
        Self {
            default_cert: Arc::new(RwLock::new(Arc::new(default_cert))),
            acme_challenge: Arc::new(Mutex::new(None)),
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

    fn start_background_updater(&self) {
        let mut interval = tokio::time::interval(Duration::from_hours(24));
        let acme_cert = Arc::clone(&self.acme_challenge);
        let default_cert = Arc::clone(&self.default_cert);
        tokio::spawn(async move {
            // wait for the listener to actually start (otherwise we'd miss the acme request)
            // technically this is still racy (it should be deterministically synchronized)
            // but this is my garden and I don't feel like pruning right now
            tokio::time::sleep(Duration::from_secs(1)).await;

            loop {
                interval.tick().await;

                let current_leaf = default_cert.read().unwrap().cert.first().unwrap().clone();
                let (remaining, cert) = x509_parser::parse_x509_certificate(&current_leaf).unwrap();
                assert!(remaining.is_empty());
                let self_signed = {
                    tracing::info!(
                        "cert issuer: {:?}, cert subject: {:?}",
                        cert.issuer,
                        cert.subject
                    );
                    cert.issuer == cert.subject
                };

                let almost_expired = {
                    let expiration = cert.validity().not_after.to_datetime();
                    let now = OffsetDateTime::now_utc();
                    let diff = expiration - now;
                    tracing::info!("{diff:?} until cert expiration");
                    let until_expiration = if diff.is_negative() {
                        std::time::Duration::from_secs(0)
                    } else {
                        diff.unsigned_abs()
                    };
                    tracing::info!("{until_expiration:?} until cert expires");
                    let days_until_expiration = until_expiration.as_secs() / (24 * 3_600);
                    days_until_expiration < 2
                };

                if self_signed || almost_expired {
                    let mut cert_order = match CertOrder::create().await {
                        Ok(order) => order,
                        Err(e) => {
                            tracing::error!("failed to create cert order: {e}");
                            continue;
                        }
                    };

                    let new_acme_cert = match cert_order.acme_cert().await {
                        Ok(cert) => cert.to_rustls(),
                        Err(e) => {
                            tracing::error!("failed to create acme cert {e}");
                            continue;
                        }
                    };
                    *acme_cert.lock().unwrap() = Some(Arc::new(new_acme_cert));

                    let new_public_cert = match cert_order.finish().await {
                        Ok(cert) => cert,
                        Err(e) => {
                            tracing::error!("failed to create new public cert: {e}");
                            continue;
                        }
                    };
                    new_public_cert.cache();
                    let new_public_cert = new_public_cert.to_rustls();
                    *default_cert.write().unwrap() = Arc::new(new_public_cert);
                }
            }
        });
        // store the task on the thing to make it harder for me to shoot myself in the
        // foot and accidentally create multiple copies of this.

        // daily, starting now
        // 1. check current cert
        // 2. if self signed or close to expiration, fetch new cert
        // 3. update default cert
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
