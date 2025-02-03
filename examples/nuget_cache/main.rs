use cacheus::{self, CacheusServer};
use simplelog::*;
use futures::join;

#[tokio::main]
async fn main()
{
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    let cacheus_fut = CacheusServer::start_from_config_file("examples/nuget_cache/config.yaml");

    join!(cacheus_fut);
}
