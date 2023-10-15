use crate::slab::Slab;
use std::clone::Clone;
use std::collections::{HashMap, LinkedList};
use std::io::{BufReader, Cursor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::{select, time};
use tokio_rustls::server::TlsStream;
use tokio_rustls::{rustls, TlsAcceptor};

#[derive(Debug)]
struct Session {
    // 正使用吗
    using: Arc<AtomicBool>,
    // 外部连接
    outcons: [Arc<Mutex<Outcon>>; bncom::_const::SESSION_CAP],
    // 下一连接位置
    next: Mutex<usize>,
    // port
    port: u16,
}

impl Session {
    fn new(port: u16) -> Session {
        let mut vc: Vec<Arc<Mutex<Outcon>>> = Vec::with_capacity(bncom::_const::SESSION_CAP);
        for _ in 0..bncom::_const::SESSION_CAP {
            vc.push(Arc::new(Mutex::new(Outcon::new())));
        }
        let vca: [Arc<Mutex<Outcon>>; bncom::_const::SESSION_CAP] = vc.try_into().unwrap();
        Session {
            using: Arc::new(AtomicBool::new(true)),
            outcons: vca,
            next: Mutex::new(0),
            port,
        }
    }
    async fn push(&self, conn: TcpStream) -> Option<usize> {
        let mut count = 0;
        loop {
            let mut next = self.next.lock().await;
            let key = (*next) & bncom::_const::SESSION_CAP_MASK;
            *next += 1;
            let mut u = self.outcons[key].lock().await;
            let start = SystemTime::now();
            let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
            let cur = since_the_epoch.as_secs();
            if u.conn.is_some() {
                // 存在连接，超时判断
                // 10秒超时
                if cur - u.start > 10 {
                    let oldconn = u.conn.take();
                    if let Some(mut c) = oldconn {
                        c.shutdown().await.unwrap();
                    }
                    u.conn = Some(conn);
                    u.start = cur;
                    return Some(key);
                }
            } else {
                u.conn = Some(conn);
                u.start = cur;
                return Some(key);
            }
            count += 1;
            if count >= bncom::_const::SESSION_CAP {
                return None;
            }
        }
    }
    // 关闭Session
    async fn close(sf: &Session) {
        sf.using.store(false, Ordering::Relaxed);
        let _ = match TcpStream::connect(format!("127.0.0.1:{}", sf.port)).await {
            Ok(secon) => secon,
            Err(_) => {
                return;
            }
        };
        // 释放资源
        for i in 0..bncom::_const::SESSION_CAP {
            let aoc = Arc::clone(&sf.outcons[i]);
            let mut v = aoc.lock().await;
            v.start = u64::MAX;
            v.conn = None;
        }
    }
}
#[derive(Debug)]
struct Outcon {
    conn: Option<TcpStream>,
    start: u64,
}

impl Outcon {
    fn new() -> Outcon {
        Outcon {
            conn: None,
            start: u64::MAX,
        }
    }
}

async fn work_for_client(
    sdr: Sender<[u8; 4]>,
    sid: u16,
    son_session: Arc<Session>,
    listener: TcpListener,
) {
    let us = Arc::clone(&son_session.using);
    log::info!("Bind {}", son_session.port);
    let p1 = (sid >> 8) as u8;
    let p2 = (sid & 0xff) as u8;

    loop {
        let apt = listener.accept().await;
        if us.load(Ordering::Relaxed) == false {
            log::info!("Port break {}", son_session.port);
            return;
        }
        match apt {
            Ok((stream, _)) => {
                // 通知客户端新连接到了
                if let Some(i) = son_session.push(stream).await {
                    sdr.send([bncom::_const::NEWSOCKET, p1, p2, i as u8])
                        .await
                        .unwrap();
                }
            }
            Err(e) => {
                log::error!("error {}", e);
                log::info!("Port break {}", son_session.port);
                return;
            }
        }
    }
}

// 关闭连接清理资源
async fn close_session(
    ids: Vec<usize>,
    sessions: Arc<RwLock<Slab<Arc<Session>>>>,
    port_sessionid_map: Arc<RwLock<HashMap<u16, usize>>>,
) {
    for id in ids {
        close_session_sigle(id, sessions.clone(), port_sessionid_map.clone()).await;
    }
}

// 关闭连接清理资源
async fn close_session_sigle(
    id: usize,
    sessions: Arc<RwLock<Slab<Arc<Session>>>>,
    port_sessionid_map: Arc<RwLock<HashMap<u16, usize>>>,
) {
    let mut sew = sessions.write().await;
    let seo = sew.remove(id);
    match seo {
        Some(sec) => {
            let mut wpsm = port_sessionid_map.write().await;
            wpsm.remove(&sec.port);
            log::info!("Close id {} port {}", id, sec.port);
            Session::close(&sec).await;
        }
        None => {}
    }
}

async fn do_start(
    session: Arc<RwLock<Slab<Arc<Session>>>>,
    port_sessionid_map: Arc<RwLock<HashMap<u16, usize>>>,
    mut stream: TlsStream<TcpStream>,
    cfg: bncom::config::Server,
) {
    // 初始化
    // START info_len info
    let mut cmd = [0u8; 8];
    if let Err(e) = stream.read_exact(&mut cmd).await {
        log::error!("Read command START error {}", e);
        return;
    }
    let info_len: u64 = ((cmd[0] as u64) << 56)
        | ((cmd[1] as u64) << 48)
        | ((cmd[2] as u64) << 40)
        | ((cmd[3] as u64) << 32)
        | ((cmd[4] as u64) << 24)
        | ((cmd[5] as u64) << 16)
        | ((cmd[6] as u64) << 8)
        | (cmd[7] as u64);
    if info_len > 1024 * 1024 {
        // 限制消息最大内存使用量 1M
        if let Err(e) = stream.write_all(&[bncom::_const::ERROR]).await {
            log::error!("Msg out of memory {}", e);
        }
        return;
    }
    let mut cmdv = vec![0u8; info_len as usize];
    if let Err(e) = stream.read_exact(&mut cmdv).await {
        log::error!("Read msg error {}", e);
        return;
    }
    let p: bncom::config::Client;
    match serde_json::from_slice::<bncom::config::Client>(&cmdv) {
        Ok(v) => {
            p = v;
        }
        Err(e) => {
            log::error!("Config is not a json {}", e);
            return;
        }
    }
    if p.map.len() == 0 {
        if let Err(e) = stream.write_all(&[bncom::_const::ERROR]).await {
            log::error!("Write command ERROR {}", e);
        }
        return;
    }
    if p.key != cfg.key {
        log::error!("Password error => {}", p.key);
        if let Err(e) = stream.write_all(&[bncom::_const::ERROR_PWD]).await {
            log::error!("Write Password error {}", e);
            return;
        }
        return;
    }
    // 检车端口是否被占用
    // 检查端口是否在规定范围
    let mut listeners: LinkedList<TcpListener> = LinkedList::new();
    for v in &p.map {
        match cfg._limit_port {
            Some((st, ed)) => {
                if v.outer > ed || v.outer < st {
                    // 端口超范围
                    if let Err(e) = stream.write_all(&[bncom::_const::ERROR_LIMIT_PORT]).await {
                        log::error!("Over port limit {}", e);
                        return;
                    }
                    return;
                }
            }
            None => {}
        }
        for i in 1..10 {
            match TcpListener::bind(format!("0.0.0.0:{}", v.outer)).await {
                Ok(listener) => {
                    listeners.push_back(listener);
                    break;
                }
                Err(_) => {
                    // 端口被占用，关闭端口
                    log::info!("Port is occupied to close {}", v.outer);
                    let mut rid = 0usize;
                    if let Some(id) = port_sessionid_map.read().await.get(&v.outer) {
                        rid = *id;
                    }
                    close_session_sigle(rid, session.clone(), port_sessionid_map.clone()).await;
                    time::sleep(time::Duration::from_secs(1)).await;
                }
            }
            if i >= 9 {
                if let Err(e) = stream.write_all(&[bncom::_const::ERROR_BUSY]).await {
                    log::error!("Port is occupied error {}", e);
                    return;
                }
                return;
            }
        }
    }
    let (s, mut r): (Sender<[u8; 4]>, Receiver<[u8; 4]>) = channel(100);
    // 客户端存入session
    // 成功建立连接时的响应
    let mut _success: Vec<u8> = Vec::with_capacity(1 + p.map.len() * 2);
    let mut ids: Vec<usize> = Vec::with_capacity(p.map.len());

    _success.push(bncom::_const::SUCCESS);
    // 客户端存入session
    for v in &p.map {
        let sec = Arc::new(Session::new(v.outer));
        let mut sew = session.write().await;
        let psmc = port_sessionid_map.clone();
        let mut psm = psmc.write().await;
        let mut sid: u16 = 0;
        if let Some(id) = sew.push(Arc::clone(&sec)) {
            sid = id as u16;
            ids.push(id);
            psm.insert(v.outer, id);
            _success.push(((id & 0xffff) > 8) as u8);
            _success.push((id & 0xff) as u8);
        } else {
            // Session初始化失败
            if let Err(e) = stream.write_all(&[bncom::_const::ERROR_SESSION_OVER]).await {
                log::error!("Session init error {}", e);
                close_session(ids, Arc::clone(&session), port_sessionid_map).await;
                return;
            }
        }

        let listener = listeners.pop_front().unwrap();
        let sd = s.clone();
        tokio::spawn(async move {
            work_for_client(sd, sid, sec, listener).await;
        });
    }

    if let Err(e) = stream.write_all(&_success).await {
        log::error!("Wait close session {}", e);
        close_session(ids, session, port_sessionid_map).await;
        return;
    }

    let (mut rs, mut ws) = tokio::io::split(stream);

    let mut heartbeat_timeout =
        time::interval(time::Duration::from_secs(bncom::_const::HEARTBEAT_TIMEOUT));
    heartbeat_timeout.tick().await;

    let mut cmd = [0u8];
    loop {
        select! {
            Some(c) = r.recv() => {
                if c[0] == bncom::_const::KILL {
                    r.close();
                    break;
                }
                let r = ws.write_all(&c).await;
                if let Err(_) = r {
                    break;
                }
            }
            n = rs.read(&mut cmd) => {
                match n {
                    Ok(0) => {
                        break;
                    }
                    Ok(_) => {
                        match cmd[0] {
                            bncom::_const::IDLE => {
                                heartbeat_timeout.reset();
                                match ws.write_u8(bncom::_const::IDLE).await {
                                    Ok(_)=>{}
                                    Err(e) => {
                                        log::error!("Write is broken {}", e);
                                    }
                                }
                            }
                            _ => {}
                        }
                    },
                    Err(_) => {
                        break;
                    }
                }
            }
            _ = heartbeat_timeout.tick() => {
                // 心跳定时器超时
                log::error!("Heartbeat overtime");
                break;
            }

        }
    }
    // 关闭连接清理资源
    close_session(ids, session, port_sessionid_map).await;
}

async fn do_newconn(session: Arc<RwLock<Slab<Arc<Session>>>>, mut stream: TlsStream<TcpStream>) {
    let mut cmd = [0u8; 3];
    if let Err(e) = stream.read_exact(&mut cmd).await {
        log::error!("Read newcon command {}", e);
        return;
    }

    let id = ((cmd[0] as usize) << 8) | (cmd[1] as usize);
    let son_index = cmd[2] as usize;
    if son_index >= bncom::_const::SESSION_CAP {
        // 下标越界
        return;
    }
    let mut ocon: Option<TcpStream> = None;
    {
        let ser = session.read().await;
        let seo = ser.get(id);
        if let Some(se) = seo {
            let outc = &se.outcons[son_index];
            let mut oc = outc.lock().await;
            ocon = oc.conn.take();
        }
    }
    if let Some(mut con) = ocon {
        let (mut outr, mut outw) = tokio::io::split(stream);
        let (mut inr, mut inw) = con.split();

        let t1 = async {
            match tokio::io::copy(&mut outr, &mut inw).await {
                Ok(_) => {}
                Err(_) => {}
            }
            if let Err(e) = inw.shutdown().await {
                log::info!("Instream close {}", e);
            }
        };
        let t2 = async {
            match tokio::io::copy(&mut inr, &mut outw).await {
                Ok(_) => {}
                Err(_) => {}
            }
            if let Err(e) = outw.shutdown().await {
                log::info!("Outstream close {}", e);
            }
        };
        tokio::join!(t1, t2);
    }
}

async fn new_socket(
    session: Arc<RwLock<Slab<Arc<Session>>>>,
    port_sessionid_map: Arc<RwLock<HashMap<u16, usize>>>,
    mut stream: TlsStream<TcpStream>,
    cfg: bncom::config::Server,
) {
    let mut cmd = [0u8];
    if let Err(e) = stream.read_exact(&mut cmd).await {
        log::error!("Read command error {}", e);
        return;
    }
    match cmd[0] {
        bncom::_const::START => {
            do_start(session, port_sessionid_map, stream, cfg).await;
        }
        bncom::_const::NEWCONN => {
            do_newconn(session, stream).await;
        }
        _ => {}
    }
}

pub async fn handle_server(server: bncom::config::Server) {
    log::info!("server start->{:#?}", server);
    let mut sessions: Slab<Arc<Session>> = Slab::new();
    sessions.set_limit_len(bncom::_const::SESSION_MAX);
    let arc_sessions = Arc::new(RwLock::new(sessions));
    let port_sessionid_map = Arc::new(RwLock::new(HashMap::<u16, usize>::new()));
    let listener = TcpListener::bind(format!("0.0.0.0:{}", server.port))
        .await
        .unwrap();

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
        let stream = match tlsacceptor.accept(stream).await {
            Ok(x) => x,
            Err(e) => {
                log::error!("{}", e);
                continue;
            }
        };

        let se = Arc::clone(&arc_sessions);
        let pm = port_sessionid_map.clone();
        let cfg = server.clone();
        tokio::spawn(async move {
            new_socket(se, pm, stream, cfg).await;
        });
    }
}
