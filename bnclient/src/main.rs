mod client;

fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let cfg = bncom::config::get_config();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(3)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            if let Some(client_cfg) = cfg.client {
                let th = tokio::spawn(async move {
                    client::handle_client(client_cfg).await;
                });
                tokio::try_join!(th).unwrap();
            }
        });
}
