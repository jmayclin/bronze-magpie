use std::{net::IpAddr, sync::Arc};

use axum::serve::IncomingStream;
use rustls::{NamedGroup, ProtocolVersion, ServerConfig, SupportedCipherSuite};
use tokio::net::TcpListener;

use crate::{cert, tls};

pub struct TlsListener {
    tcp: TcpListener,
    tls: Arc<ServerConfig>,
}

impl TlsListener {
    pub async fn new() -> Self {
        let cert_resolver = cert::CertResolver::new();
        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(cert_resolver));
        let tls = Arc::new(server_config);

        let is_dev = std::env::var("STAGE")
            .map(|stage| stage == "DEV")
            .unwrap_or(false);
        let port = if is_dev { 3000 } else { 443 };

        let tcp = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .unwrap();
        Self { tcp, tls }
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (transport, addr) = match self.tcp.accept().await {
                Ok((stream, addr)) => (stream, addr),
                Err(e) => {
                    tracing::error!("{e}");
                    continue;
                }
            };

            let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::clone(&self.tls));
            let tls_stream = match tls_acceptor.accept(transport).await {
                Ok(tls) => tls,
                Err(e) => {
                    tracing::error!("{e}");
                    continue;
                }
            };

            tracing::info!("accepted tcp connection from {addr}");
            return (tls_stream, addr);
        }
    }

    fn local_addr(&self) -> tokio::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

#[derive(Debug, Clone)]
pub struct TlsConnectionInfo {
    client_ip: IpAddr,
    client_port: u16,
    tls_version: ProtocolVersion,
    tls_cipher: SupportedCipherSuite,
    tls_group: NamedGroup,
}

impl axum::extract::connect_info::Connected<IncomingStream<'_, TlsListener>> for TlsConnectionInfo {
    fn connect_info(stream: IncomingStream<'_, TlsListener>) -> Self {
        let io = stream.io();
        let (tcp, tls) = io.get_ref();
        let client_addr = tcp.peer_addr().unwrap();
        let client_ip = client_addr.ip();
        let client_port = client_addr.port();
        let tls_version = tls.protocol_version().unwrap();
        let tls_cipher = tls.negotiated_cipher_suite().unwrap();
        let tls_group = tls.negotiated_key_exchange_group().unwrap().name();
        Self {
            client_ip,
            client_port,
            tls_version,
            tls_cipher,
            tls_group,
        }
    }
}
