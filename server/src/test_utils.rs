use std::{collections::VecDeque, sync::Arc};

use rustls::{
    ClientConfig, ClientConnection, ServerConfig, ServerConnection,
    pki_types::{DnsName, ServerName},
};

#[derive(Debug, Default)]
struct Io {
    client_tx_stream: VecDeque<u8>,
    server_tx_stream: VecDeque<u8>,
}

#[derive(Debug)]
struct TestPair {
    client: ClientConnection,
    server: ServerConnection,
    io: Io,
}

impl TestPair {
    fn from_configs(client: Arc<ClientConfig>, server: Arc<ServerConfig>) -> Self {
        let hostname: DnsName = "bronzemagpie.com".try_into().unwrap();
        let client = ClientConnection::new(client, ServerName::DnsName(hostname)).unwrap();
        let server = ServerConnection::new(server).unwrap();
        let io = Io::default();
        Self { client, server, io }
    }

    fn handshake(&mut self) -> anyhow::Result<()> {
        let mut counter = 10;
        while self.client.is_handshaking() && self.server.is_handshaking() {
            counter -= 1;
            if counter == 0 {
                panic!("too many loops");
            }
            // TODO: make the partial read/write handling less ugly
            while !self.io.server_tx_stream.is_empty() {
                self.client.read_tls(&mut self.io.server_tx_stream).unwrap();
            }
            self.client.process_new_packets().unwrap();

            while self.client.wants_write() {
                self.client
                    .write_tls(&mut self.io.client_tx_stream)
                    .unwrap();
            }
            while !self.io.client_tx_stream.is_empty() {
                self.server.read_tls(&mut self.io.client_tx_stream).unwrap();
            }
            self.server.process_new_packets().unwrap();
            while self.server.wants_write() {
                self.server
                    .write_tls(&mut self.io.server_tx_stream)
                    .unwrap();
            }
            // println!(
            //     "c read {:?}, c write {:?}, s read {:?}, s write {:?}",
            //     self.client.wants_read(),
            //     self.client.wants_write(),
            //     self.server.wants_read(),
            //     self.server.wants_write()
            // );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rustls::RootCertStore;
    use tracing::Level;
    use tracing_subscriber::EnvFilter;

    use crate::cert::CertResolver;

    use super::*;

    #[test]
    fn handshakes() {
        // construct a subscriber that prints formatted traces to stdout
        // tracing_log::LogTracer::init().unwrap();
        let _ = tracing_log::LogTracer::init();

        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("trace,rustls=trace"))
            .with_test_writer()
            .try_init();
        tracing::info!("DO I SEE THIS?");

        let cert_resolver = CertResolver::new();
        let root = cert_resolver
            .default_cert
            .read()
            .unwrap()
            .cert
            .first()
            .unwrap()
            .clone();
        let server = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(cert_resolver));

        let mut ca = RootCertStore::empty();
        ca.add(root);

        let client = ClientConfig::builder()
            .with_root_certificates(Arc::new(ca))
            .with_no_client_auth();

        let mut pair = TestPair::from_configs(Arc::new(client), Arc::new(server));
        pair.handshake().unwrap()
    }
}
