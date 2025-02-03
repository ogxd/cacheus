use cacheus::{self, CacheusServer};
use futures::join;

#[tokio::main]
async fn main()
{
    let cacheus_fut = CacheusServer::start_from_config_file("examples/nuget_cache/config.yaml");

    join!(cacheus_fut);
}
