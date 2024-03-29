use std::io;

use qtunnel::args::parse_args;
use qtunnel::{client, server};
use tokio::signal;

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let cfg = parse_args("qtunnel").expect("invalid config");
    log::info!("config: \n{}", toml::ser::to_string_pretty(&cfg).unwrap());

    if let Some(cfg) = cfg.client {
        tokio::spawn(async {
            if let Err(err) = client::run(cfg).await {
                log::error!("client fail: {:?}", err);
            }
        });
    }
    if let Some(cfg) = cfg.server {
        tokio::spawn(async {
            if let Err(err) = server::run(cfg).await {
                log::error!("client fail: {:?}", err);
            }
        });
    }

    signal::ctrl_c().await.expect("failed to listen for event");
    Ok(())
}
