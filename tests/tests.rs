include!("../proto/helloworld.rs");

use std::{clone, time::Duration};

use greeter_client::GreeterClient;
use greeter_server::{Greeter, GreeterServer};
use cacheus::{self, CacheusServer};
use tokio::sync::oneshot;
use tonic::{metadata::MetadataMap, metadata::MetadataValue, transport::Server, Extensions, Request, Response, Status};
use warp::Filter;

#[derive(Debug, Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter
{
    async fn say_hello(&self, request: Request<HelloRequest>) -> Result<Response<HelloReply>, Status>
    {
        println!("Got a request: {:?}", request);

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
        Self::start(Server::builder()
            .add_service(GreeterServer::new(MyGreeter::default()))
            .serve("127.0.0.1:3002".parse().unwrap()))
    }

    pub fn new_cacheus() -> Self
    {
        Self::start(CacheusServer::start_from_config_file("tests/config.yaml"))
    }

    fn start<F>(fut: F) -> Self
        where F : core::future::Future + Send + 'static
    {
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let server_handle = tokio::spawn(async move {
        tokio::select! {
            _ = fut => {},
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

use simplelog::*;

#[tokio::test]
async fn grpc()
{
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    let server = TestServer::new_grpc();
    let cacheus = TestServer::new_cacheus();

    // Warmup
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut client = GreeterClient::connect("http://127.0.0.1:3001").await.unwrap();

    let mut metadata1 = MetadataMap::new();
    metadata1.insert("x-target-host", "127.0.0.1:3002".parse().unwrap());

    let request1 = tonic::Request::from_parts(
        metadata1,
        Extensions::default(),
        HelloRequest { name: "Tonic".into() });

    let mut metadata2 = MetadataMap::new();
    metadata2.insert("x-target-host", "127.0.0.1:3002".parse().unwrap());

    let request2 = tonic::Request::from_parts(
        metadata2,
        Extensions::default(),
        HelloRequest { name: "Mom".into() });

    let mut metadata3 = MetadataMap::new();
    metadata3.insert("x-target-host", "127.0.0.1:3002".parse().unwrap());

    let request3 = tonic::Request::from_parts(
        metadata3,
        Extensions::default(),
        HelloRequest { name: "Dad".into() });
    
    // let response1 = client.say_hello(request1).await.unwrap();
    // let response2 = client.say_hello(request2).await.unwrap();
    // let response3 = client.say_hello(request3).await.unwrap();

    let mut c1 = client.clone();
    let mut c2 = client.clone();
    let mut c3 = client.clone();

    let response1 = c1.say_hello(request1);
    let response2 = c2.say_hello(request2);
    let response3 = c3.say_hello(request3);

    tokio::join!(response1, response2, response3);

    server.shutdown().await;
    cacheus.shutdown().await;

    // Check grpc message content
    // assert!(response1.get_ref().message == "Hello Tonic!");
    // assert!(response2.get_ref().message == "Hello Mom!");
    // assert!(response3.get_ref().message == "Hello Dad!");
}

// #[tokio::test]
// async fn https_external()
// {
    

//     let cacheus = TestServer::new_cacheus();

//     // Warmup
//     tokio::time::sleep(Duration::from_secs(1)).await;


//     server.shutdown().await;
//     cacheus.shutdown().await;

//     // Check grpc message content
//     assert!(response.get_ref().message == "Hello Tonic!");
// }

// "https://httpbin.org/get"