mod server;
mod slab;

fn main() {
    let mut cfgname = String::new();
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        cfgname.push_str("config.json");
    } else {
        cfgname.push_str(&args[1]);
    }
    let cfg = bncom::config::Config::from_file(&cfgname).unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(3)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            if let Some(server_cfg) = cfg.server {
                let th = tokio::spawn(async move {
                    server::handle_server(server_cfg).await;
                });
                tokio::try_join!(th).unwrap();
            }
        });
}
