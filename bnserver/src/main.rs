mod server;
mod slab;

fn main() {
    let cfg = bncom::config::get_config();
    simple_logger::init_with_level(log::Level::Info).unwrap();
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
