use cacheus::{self, CacheusServer};
use warp::Filter;
use futures::join;

#[tokio::main]
async fn main()
{
    let echo = warp::any().map(|| "Hello, World!");

    let server_fut = warp::serve(echo)
        .run(([127, 0, 0, 1], 3002));

    let cacheus_fut = CacheusServer::start_from_config_file("benches/qps_http/config.yaml");

    join!(server_fut, cacheus_fut);
}
