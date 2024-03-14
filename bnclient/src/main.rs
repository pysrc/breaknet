use std::{fs::File, io::{BufReader, Cursor, Read}, num::NonZeroU64, sync::Arc};

use serde::{Deserialize, Serialize};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, time};
use tokio_rustls::{rustls::{self, ServerName}, webpki, TlsConnector};


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Iomap {
    pub inner: String,
    pub outer: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub server: String,
    #[serde(rename = "ssl-cert")]
    pub ssl_cert: String,
    pub map: Vec<Iomap>,
}

impl Config {
    fn from_file(filename: &str) -> Self {
        let f = File::open(filename);
        match f {
            Ok(mut file) => {
                let mut c = String::new();
                file.read_to_string(&mut c).unwrap();
                let cfg: Config = serde_yaml::from_str(&c).unwrap();
                cfg
            }
            Err(e) => {
                panic!("error {}", e)
            }
        }
    }
}

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

async fn run(
    iomap_str: String, 
    cfg: Config,
    connector: TlsConnector,
    domain: ServerName,
) {
    log::info!("client start->\n{:#?}", cfg);
    let conn = TcpStream::connect(cfg.server).await.unwrap();
    let stream = connector.connect(domain.clone(), conn).await.unwrap();
    
    let (mux_connector, mut mux_acceptor, mux_worker) = async_smux::MuxBuilder::client().with_keep_alive_interval(NonZeroU64::new(30).unwrap()).with_connection(stream).build();
    tokio::spawn(mux_worker);
    let mut _cfg_stream = mux_connector.connect().unwrap();
    _cfg_stream.write_u16(iomap_str.len() as u16).await.unwrap();
    _cfg_stream.write_all(iomap_str.as_bytes()).await.unwrap();
    _cfg_stream.flush().await.unwrap();
    _cfg_stream.shutdown().await.unwrap();
    loop {
        let mut _stream = mux_acceptor.accept().await.unwrap();
        tokio::spawn(async move {
            let _len = _stream.read_u16().await.unwrap() as usize;
            let mut _dst_data = vec![0u8; _len];
            _stream.read_exact(&mut _dst_data).await.unwrap();
            // 解析地址
            let dst = String::from_utf8_lossy(&_dst_data).to_string();
            log::info!("{} open dst {}", line!(), dst);
            match TcpStream::connect(&dst).await {
                Ok(mut _inner_stream) => {
                    log::info!("{} open dst success {}", line!(), dst);
                    _ = tokio::io::copy_bidirectional(&mut _stream, &mut _inner_stream).await;
                }
                Err(e) => {
                    log::error!("{} -> {} open dst error {}", line!(), dst, e);
                    _ = _stream.shutdown().await;
                }
            }
            log::info!("{} close dst {}", line!(), dst);
        });
    }
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let cfg = Config::from_file("bnclient-config.yml");
    let iomap_str = serde_yaml::to_string(&cfg.map).unwrap();

    let mut cert = Vec::<u8>::new();
    match File::open(&cfg.ssl_cert) {
        Ok(mut f) => f.read_to_end(&mut cert).unwrap(),
        Err(e) => panic!("{}", e)
    };
    let (connector, domain) = tls_cert(&cert, "breaknet");

    loop {
        let cfg = cfg.clone();
        let iomap_str = iomap_str.clone();
        let connector = connector.clone();
        let domain = domain.clone();
        let rt = tokio::spawn(async move {
            run(iomap_str, cfg, connector, domain).await;
        });
        _ = rt.await;
        time::sleep(time::Duration::from_secs(1)).await;
    }
}
