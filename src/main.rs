use cacheus::CacheusServer;
use simplelog::*;

#[tokio::main]
async fn main()
{
    println!(" █████╗  █████╗  █████╗ ██╗  ██╗███████╗██╗   ██╗ ██████╗");
    println!("██╔══██╗██╔══██╗██╔══██╗██║  ██║██╔════╝██║   ██║██╔════╝");
    println!("██║  ╚═╝███████║██║  ╚═╝███████║█████╗  ██║   ██║╚█████╗ ");
    println!("██║  ██╗██╔══██║██║  ██╗██╔══██║██╔══╝  ██║   ██║ ╚═══██╗");
    println!("╚█████╔╝██║  ██║╚█████╔╝██║  ██║███████╗╚██████╔╝██████╔╝");
    println!(" ╚════╝ ╚═╝  ╚═╝ ╚════╝ ╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚═════╝ ");
    println!();

    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    CacheusServer::start_from_config_file("/etc/cacheus.yml").await;
}
