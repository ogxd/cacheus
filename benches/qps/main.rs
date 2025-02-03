include!("../../proto/helloworld.rs");

use greeter_server::{Greeter, GreeterServer};
use cacheus::{self, CacheusServer};
use tokio::sync::oneshot;
use tonic::{transport::Server, Request, Response, Status};

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter
{
    async fn say_hello(&self, request: Request<HelloRequest>) -> Result<Response<HelloReply>, Status>
    {
        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };

        Ok(Response::new(reply))
    }
}

pub struct TestServer
{
    server_handle: tokio::task::JoinHandle<()>,
    shutdown_sender: oneshot::Sender<()>,
}

impl TestServer
{
    pub fn new_grpc() -> Self
    {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server_handle = tokio::spawn(async move {
            let server = Server::builder()
                .add_service(GreeterServer::new(MyGreeter::default()))
                .serve("127.0.0.1:3002".parse().unwrap());
            tokio::select! {
                _ = server => {},
                _ = shutdown_receiver => {
                    // Shutdown signal received
                    println!("Shutting down the server...");
                }
            }
        });
        Self {
            server_handle,
            shutdown_sender,
        }
    }

    pub async fn shutdown(self)
    {
        self.shutdown_sender.send(()).unwrap();
        self.server_handle.await.unwrap();
    }
}

#[tokio::main]
async fn main()
{
    let server = TestServer::new_grpc();

    CacheusServer::start_from_config_file("benches/qps/config.yaml").await;

    server.shutdown().await;
}
