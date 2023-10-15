use std::collections::HashMap;
use std::io::BufReader;
use std::io::Cursor;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::select;
use tokio::time;
use tokio_rustls::rustls;
use tokio_rustls::rustls::OwnedTrustAnchor;
use tokio_rustls::rustls::ServerName;
use tokio_rustls::webpki;
use tokio_rustls::TlsConnector;

async fn work_for_server(
    connector: TlsConnector,
    server_name: ServerName,
    svr: &str,
    inner: &str,
    port_session: &[u8],
) {
    // 连内网
    let conn = TcpStream::connect(inner).await;
    match conn {
        Ok(mut incon) => {
            // 发外网指令
            let conn = TcpStream::connect(svr).await;
            match conn {
                Ok(outcon) => {
                    let mut outcon = connector.connect(server_name, outcon).await.unwrap();
                    // swap
                    if let Err(e) = outcon.write_all(port_session).await {
                        log::error!("error {}", e);
                        return;
                    }

                    let (mut outr, mut outw) = tokio::io::split(outcon);
                    let (mut inr, mut inw) = incon.split();

                    let t1 = async {
                        match tokio::io::copy(&mut outr, &mut inw).await {
                            Ok(_) => {}
                            Err(_) => {}
                        }
                        if let Err(e) = inw.shutdown().await {
                            log::error!("{}", e);
                        }
                    };
                    let t2 = async {
                        match tokio::io::copy(&mut inr, &mut outw).await {
                            Ok(_) => {}
                            Err(_) => {}
                        }
                        if let Err(e) = outw.shutdown().await {
                            log::error!("{}", e);
                        }
                    };
                    tokio::join!(t1, t2);
                }
                Err(e) => {
                    log::error!("error {}", e);
                }
            }
        }
        Err(e) => {
            log::error!("error {}", e);
        }
    }
}

pub fn tls_cert(cert: &[u8], name: &str) -> (TlsConnector, ServerName) {
    let cs = Cursor::new(cert);
    let mut br = BufReader::new(cs);
    let certs = rustls_pemfile::certs(&mut br).unwrap();
    let trust_anchors = certs.iter().map(|cert| {
        let ta = webpki::TrustAnchor::try_from_cert_der(&cert[..]).unwrap();
        OwnedTrustAnchor::from_subject_spki_name_constraints(
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

pub async fn ever_handle(
    client: bncom::config::Client,
    connector: TlsConnector,
    server_name: ServerName,
) {
    let conn = TcpStream::connect(&client.server).await;
    match conn {
        Ok(con) => {
            let mut con = connector.connect(server_name.clone(), con).await.unwrap();
            // 发送START指令
            let sjson = client.clone();
            let json_str = serde_json::to_string(&sjson).expect("Json error");
            let info = json_str.as_bytes();
            let info_len = info.len() as u64;
            if info_len > 1024 * 1024 {
                // 限制消息最大内存使用量 1M
                log::error!("Config over 1024 * 1024");
                return;
            }
            let tob = |b: u8| -> u8 { ((info_len >> (b * 8)) & 0xff) as u8 };
            let lenb = [
                tob(7),
                tob(6),
                tob(5),
                tob(4),
                tob(3),
                tob(3),
                tob(1),
                tob(0),
            ];
            let mut start_msg: Vec<u8> = Vec::new();
            start_msg.push(bncom::_const::START);
            start_msg.extend(lenb);
            start_msg.extend(info);
            con.write_all(&start_msg).await.expect("Send error");
            let mut res = [0u8];
            if let Err(e) = con.read_exact(&mut res).await {
                log::error!("Read 1 from server is broken {}", e);
                return;
            }
            let mut session_map: HashMap<u16, String> = HashMap::with_capacity(client.map.len());
            match res[0] {
                bncom::_const::SUCCESS => {
                    // 处理成功，获取会话ID
                    let mut ids: Vec<u8> = Vec::with_capacity(client.map.len() * 2);
                    for _ in 0..client.map.len() * 2 {
                        ids.push(0);
                    }
                    if let Err(e) = con.read_exact(&mut ids).await {
                        log::error!("Read 2 from server is broken {}", e);
                        return;
                    }
                    for i in 0..client.map.len() {
                        session_map.insert(
                            ((ids[i * 2] as u16) << 8) | (ids[i * 2 + 1] as u16),
                            client.map[i].inner.clone(),
                        );
                    }
                }
                bncom::_const::ERROR => {
                    log::error!("Server error ERROR");
                    return;
                }
                bncom::_const::ERROR_BUSY => {
                    log::error!("Server error ERROR_BUSY");
                    return;
                }
                bncom::_const::ERROR_PWD => {
                    log::error!("Server error ERROR_PWD");
                    return;
                }
                bncom::_const::ERROR_LIMIT_PORT => {
                    log::error!("Port error ERROR_LIMIT_PORT");
                    return;
                }
                bncom::_const::ERROR_SESSION_OVER => {
                    log::error!("Port error ERROR_SESSION_OVER");
                    return;
                }
                _ => {
                    log::error!("Password error {}", res[0]);
                    return;
                }
            }

            log::info!("Client is running");
            let mut cmd = [0u8; 4];
            let (mut rcon, mut wcon) = tokio::io::split(con);
            let mut heartbeat =
                time::interval(time::Duration::from_secs(bncom::_const::HEARTBEAT_TIME));
            let mut heartbeat_timeout =
                time::interval(time::Duration::from_secs(bncom::_const::HEARTBEAT_TIMEOUT));
            heartbeat_timeout.tick().await;
            loop {
                select! {
                    res = rcon.read(&mut cmd) => {
                        match res {
                            Ok(si) => {
                                if si == 0 {
                                    log::error!("Maybe break");
                                    return;
                                }
                                match cmd[0] {
                                    bncom::_const::NEWSOCKET => {
                                        let id = ((cmd[1] as u16) << 8) | (cmd[2] as u16);
                                        let st = session_map.get(&id);
                                        if let Some(rinner) = st {
                                            let inner = rinner.clone();
                                            let svr = client.server.clone();
                                            let connector = connector.clone();
                                            let server_name = server_name.clone();
                                            tokio::spawn(async move {
                                                work_for_server(
                                                    connector,
                                                    server_name,
                                                    &svr,
                                                    &inner,
                                                    &[bncom::_const::NEWCONN, cmd[1], cmd[2], cmd[3]],
                                                )
                                                .await;
                                            });
                                        }
                                    }
                                    bncom::_const::IDLE => {
                                        // 收到echo心跳
                                        heartbeat_timeout.reset();
                                    }
                                    _ => {}
                                }
                            }
                            Err(e) => {
                                log::error!("Read is broken {}", e);
                                return;
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        match wcon.write_u8(bncom::_const::IDLE).await {
                            Ok(_)=>{}
                            Err(e) => {
                                log::error!("Write is broken {}", e);
                                return;
                            }
                        }
                    }
                    _ = heartbeat_timeout.tick() => {
                        // 心跳定时器超时
                        log::error!("Heartbeat overtime");
                        return;
                    }
                }
            }
        }
        Err(e) => {
            log::error!("Open connection is broken {}", e);
        }
    }
}

pub async fn handle_client(client: bncom::config::Client) {
    log::info!("client start->{:#?}", client);
    let (connector, server_name) =
        tls_cert(include_bytes!("../../resources/user-cert.pem"), "breaknet");
    loop {
        log::info!("Start connect server");
        ever_handle(client.clone(), connector.clone(), server_name.clone()).await;
        time::sleep(time::Duration::from_secs(1)).await;
    }
}
