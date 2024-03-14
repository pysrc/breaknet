use std::{fs::File, io::{BufReader, Cursor, Read}, sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Arc}};

use serde::{Deserialize, Serialize};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener, select, time};
use tokio_rustls::{rustls, TlsAcceptor};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Iomap {
    pub inner: String,
    pub outer: String,
}


#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    bind: String,
    #[serde(rename = "ssl-cert")]
    ssl_cert: String,
    #[serde(rename = "ssl-key")]
    ssl_key: String,
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

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let cfg = Config::from_file("bnserver-config.yml");
    let mut cert = Vec::<u8>::new();
    match File::open(&cfg.ssl_cert) {
        Ok(mut f) => f.read_to_end(&mut cert).unwrap(),
        Err(e) => panic!("{}", e)
    };
    let mut key = Vec::<u8>::new();
    match File::open(&cfg.ssl_key) {
        Ok(mut f) => f.read_to_end(&mut key).unwrap(),
        Err(e) => panic!("{}", e)
    };

    log::info!("server start->\n{:#?}", cfg);
    let listener = TcpListener::bind(&cfg.bind).await.unwrap();
    
    let pubcs = Cursor::new(cert);
    let mut br = BufReader::new(pubcs);
    let cetrs = rustls_pemfile::certs(&mut br).unwrap();
    let prics = Cursor::new(key);
    let mut brk = BufReader::new(prics);
    let keys = rustls_pemfile::pkcs8_private_keys(&mut brk).unwrap();
    let certificate = rustls::Certificate(cetrs[0].clone());
    let private_key = rustls::PrivateKey(keys[0].clone());
    let cert_chain = vec![certificate];
    let tlsconfig = Arc::new(
        rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .unwrap(),
    );
    let tlsacceptor = TlsAcceptor::from(tlsconfig);
    let global_id = Arc::new(AtomicU64::new(0));
    while let Ok((stream, _)) = listener.accept().await {
        let tlsacceptor = tlsacceptor.clone();
        let global_id = global_id.clone();
        tokio::spawn(async move {
            let stream = match tlsacceptor.accept(stream).await {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{}", e);
                    return;
                }
            };
            let (mux_connector, mut mux_acceptor, mux_worker) = async_smux::MuxBuilder::server().with_connection(stream).build();
            let _worker = tokio::spawn(mux_worker);
            let mut _cfg_stream = mux_acceptor.accept().await.unwrap();
            let _len = _cfg_stream.read_u16().await.unwrap();
            let mut iomap_vec = vec![0u8; _len as usize];
            _cfg_stream.read_exact(&mut iomap_vec).await.unwrap();
            _ = _cfg_stream.shutdown().await;
            let _iomap: Vec<Iomap> = serde_yaml::from_slice(&iomap_vec).unwrap();
            log::info!("new client \n{:#?}", _iomap);

            let run = Arc::new(AtomicBool::new(true));
            for iomap in _iomap {
                let run = run.clone();
                let mux_connector = mux_connector.clone();
                let global_id = global_id.clone();
                tokio::spawn(async move {
                    let lis = TcpListener::bind(&iomap.outer).await.unwrap();
                    let _len = iomap.inner.len() as u16;
                    let frd = iomap.inner.as_bytes();
                    while run.load(Ordering::Relaxed) {
                        select! {
                            _ = time::sleep(time::Duration::from_secs(1)) => {},
                            _income = lis.accept() => {
                                match _income {
                                    Ok((mut _conn, _)) => {
                                        // 新通道
                                        let _gid = global_id.fetch_add(1, Ordering::Relaxed);
                                        log::info!("open dst: {} id: {}", iomap.inner, _gid);
                                        let mut _mux_stream = mux_connector.connect().unwrap();
                                        _mux_stream.write_u16(_len).await.unwrap();
                                        _mux_stream.write_all(frd).await.unwrap();
                                        tokio::spawn(async move {
                                            _ = tokio::io::copy_bidirectional(&mut _conn, &mut _mux_stream).await;
                                            log::info!("close id: {}", _gid);
                                        });
                                    }
                                    Err(e) => {
                                        log::error!("{} -> {}", line!(), e);
                                    }
                                }
                            }
                        }
                        
                    }
                    log::info!("{} break port {}", line!(), iomap.outer);
                });
            }
            _ = _worker.await;
            run.store(false, Ordering::Relaxed);
            log::info!("end");
        });
    }
}
