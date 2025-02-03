use cacheus::CacheusServer;

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

    CacheusServer::start_from_config_file("/etc/cacheus.yml").await;
}
