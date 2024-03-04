use std::{io::{BufReader, Cursor}, sync::Arc};

use bncom::config::Config;
use channel_mux_with_stream::{bicopy, cmd, server::{MuxServer, StreamMuxServer}};
use tokio::{net::TcpStream, time};
use tokio_rustls::{rustls::{self, ServerName}, webpki, TlsConnector};

pub fn tls_cert(cert: &[u8], name: &str) -> (TlsConnector, ServerName) {
    let cs = Cursor::new(cert);
    let mut br = BufReader::new(cs);
    let certs = rustls_pemfile::certs(&mut br).unwrap();
    let trust_anchors = certs.iter().map(|cert| {
        let ta = webpki::TrustAnchor::try_from_cert_der(&cert[..]).unwrap();
        rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    });
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.add_server_trust_anchors(trust_anchors);
    let config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = rustls::ServerName::try_from(name).unwrap();
    (connector, server_name)
}

async fn run(cfg_str: String, cfg: Config) {
    let client_cfg = cfg.client.unwrap();
    log::info!("client start->{:#?}", client_cfg);
    let (connector, server_name) = tls_cert(include_bytes!("../../resources/user-cert.pem"), "breaknet");
    let conn = TcpStream::connect(client_cfg.server).await.unwrap();
    let stream = connector.connect(server_name.clone(), conn).await.unwrap();
    let (mut mux_server, _) = StreamMuxServer::init(stream);
    let (id, _, send, mut vec_pool) = mux_server.accept_channel().await.unwrap();
    let mut first = vec_pool.get().await;
    first.extend_from_slice(cfg_str.as_bytes());
    send.send((cmd::PKG, id, Some(first))).unwrap();
    send.send((cmd::BREAK, id, None)).unwrap();

    loop {
        let (id, mut recv, send, vec_pool) = if let Some(_t) = mux_server.accept_channel().await {
            _t
        } else {
            log::info!("{} stream close.", line!());
            return;
        };
        tokio::spawn(async move {
            let _data = match recv.recv().await {
                Some(_d) => _d,
                None => {
                    log::info!("{} recv close {}", line!(), id);
                    return;
                }
            };
            // 解析地址
            let dst = String::from_utf8_lossy(&_data).to_string();
            log::info!("{} open dst {}", line!(), dst);
            match TcpStream::connect(&dst).await {
                Ok(stream) => {
                    log::info!("{} open dst success {}", line!(), dst);
                    bicopy(id, recv, send, stream, vec_pool.clone()).await;
                }
                Err(e) => {
                    log::error!("{} -> {} open dst error {}", line!(), dst, e);
                    send.send((cmd::BREAK, id, None)).unwrap();
                }
            }
            log::info!("{} close dst {}", line!(), dst);
        });
    }
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let (cfg_str, cfg) = bncom::config::get_config();
    loop {
        let cfg_str = cfg_str.clone();
        let cfg = cfg.clone();
        let rt = tokio::spawn(async move {
            run(cfg_str, cfg).await;
        });
        _ = rt.await;
        time::sleep(time::Duration::from_secs(1)).await;
    }
}
