use std::{fs::File, io::{BufReader, Cursor, Read}, sync::{atomic::{AtomicBool, Ordering}, Arc}};

use channel_mux_with_stream::{bicopy, client::{MuxClient, StreamMuxClient}, cmd};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, select, sync::mpsc::unbounded_channel, time};
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

    while let Ok((stream, _)) = listener.accept().await {
        let tlsacceptor = tlsacceptor.clone();
        tokio::spawn(async move {
            let stream = match tlsacceptor.accept(stream).await {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{}", e);
                    return;
                }
            };
            let (mut mux_client, stop_recv) = StreamMuxClient::init(stream);
            let (id, mut recv, send, _) = mux_client.new_channel().await;
            let iomap_op = recv.recv().await;
            if iomap_op == None {
                return;
            }
            send.send((cmd::BREAK, id, None)).await.unwrap();
            let iomap_vec = iomap_op.unwrap();
            let _iomap: Vec<Iomap> = serde_yaml::from_slice(&iomap_vec).unwrap();
            log::info!("new client \n{:#?}", _iomap);

            let run = Arc::new(AtomicBool::new(true));
            let (income_send, mut income_recv) = unbounded_channel();
            for iomap in _iomap {
                let run = run.clone();
                let income_send = income_send.clone();
                tokio::spawn(async move {
                    let lis = TcpListener::bind(&iomap.outer).await.unwrap();
                    while run.load(Ordering::Relaxed) {
                        select! {
                            _ = time::sleep(time::Duration::from_secs(1)) => {},
                            _income = lis.accept() => {
                                match _income {
                                    Ok((conn, _)) => {
                                        // 新通道
                                        income_send.send((iomap.inner.clone(), conn)).unwrap();
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
            let st = tokio::spawn(async move {
                loop {
                    match income_recv.recv().await {
                        Some((addr, stream)) => {
                            let (id, recv, send, mut vec_pool) = mux_client.new_channel().await;
                            tokio::spawn(async move {
                                let mut data = vec_pool.get().await;
                                data.extend_from_slice(addr.as_bytes());
                                send.send((cmd::PKG, id, Some(data))).await.unwrap();
                                bicopy(id, recv, send, stream, vec_pool).await;
                            });
                        }
                        None => {
                            log::error!("{}->None", line!());
                            return;
                        }
                    }
                }
            });
            _ = stop_recv.await;
            run.store(false, Ordering::Relaxed);
            st.abort();
        });
    }
}
