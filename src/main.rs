use simplelog::*;

use risu::RisuServer;

#[tokio::main]
async fn main() {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Warn,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    RisuServer::start_from_config_file("/etc/risu.yml").await;
}
