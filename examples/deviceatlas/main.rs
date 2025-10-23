use cacheus::{self, CacheusServer};
use futures::join;
use log::info;
use tokio::signal;

#[tokio::main]
async fn main()
{
    let cacheus_fut = CacheusServer::start_from_config_file("examples/deviceatlas/config.yaml");

    join!(cacheus_fut);

    // Wait for SIGTERM or SIGINT
    signal::ctrl_c().await.expect("failed to listen for ctrl_c");

    info!("Shutting down Cacheus server...");
}
