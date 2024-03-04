use std::{io::{BufReader, Cursor}, sync::{atomic::{AtomicBool, Ordering}, Arc}};

use channel_mux_with_stream::{bicopy, client::{MuxClient, StreamMuxClient}, cmd};
use tokio::{net::TcpListener, select, sync::mpsc::unbounded_channel, time};
use tokio_rustls::{rustls, TlsAcceptor};
#[tokio::main]
async fn main() {
    let (_, cfg) = bncom::config::get_config();
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let server_cfg = cfg.server.unwrap();
    log::info!("server start->{:#?}", server_cfg);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", server_cfg.port)).await.unwrap();
    
    let pubcs = Cursor::new(include_bytes!("../../resources/user-cert.pem"));
    let mut br = BufReader::new(pubcs);
    let cetrs = rustls_pemfile::certs(&mut br).unwrap();
    let prics = Cursor::new(include_bytes!("../../resources/user-key.pem"));
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
        let server_cfg = server_cfg.clone();
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
            let cfg_bytes = recv.recv().await;
            if cfg_bytes == None {
                return;
            }
            send.send((cmd::BREAK, id, None)).unwrap();
            let cfg_bytes = cfg_bytes.unwrap();
            let cfg = bncom::config::Config::from_str(&String::from_utf8_lossy(&cfg_bytes)).unwrap();
            let client_cfg = cfg.client.unwrap();
            if client_cfg.key != server_cfg.key {
                log::error!("{} key is not right.", client_cfg.key);
                return;
            }
            let run = Arc::new(AtomicBool::new(true));
            let (income_send, mut income_recv) = unbounded_channel();
            for iomap in client_cfg.map {
                let run = run.clone();
                let income_send = income_send.clone();
                tokio::spawn(async move {
                    let lis = TcpListener::bind(format!("0.0.0.0:{}", iomap.outer)).await.unwrap();
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
                                send.send((cmd::PKG, id, Some(data))).unwrap();
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
