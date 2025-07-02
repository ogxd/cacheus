#[macro_use]
extern crate log;

mod buffered_body;
mod caches;
mod collections;
// pub mod config;
pub mod config;
mod executor;
mod metrics;
mod status;
// mod server;

use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

pub use caches::*;
pub use collections::*;
pub use config::Configuration;
use buffered_body::BufferedBody;
use executor::TokioExecutor;
use futures::join;
use hyper::body::Incoming;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use log::LevelFilter;
use serde::de::StdError;
use metrics::Metrics;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use status::Status;
use tokio::net::TcpListener;
use crate::config::CacheConfig;

pub struct CacheusServer
{
    configuration: Configuration,
    caches: HashMap<String, (CacheConfig, ShardedCache<u128, Response<BufferedBody>>)>,
    metrics: Metrics,
    client: Client<HttpsConnector<HttpConnector>, BufferedBody>,
}

impl CacheusServer
{
    pub async fn start_from_config_str(config_str: &str)
    {
        let configuration: Configuration =
            serde_yaml::from_str::<Configuration>(config_str).expect("Could not parse configuration file");
        CacheusServer::start(configuration).await.unwrap();
    }

    pub async fn start_from_config_file(config_file: &str)
    {
        let contents = std::fs::read_to_string(config_file).expect("Could not find configuration file");
        let configuration: Configuration =
            serde_yaml::from_str::<Configuration>(&contents).expect("Could not parse configuration file");
        CacheusServer::start(configuration).await.unwrap();
    }

    pub async fn start(configuration: Configuration) -> Result<(), std::io::Error>
    {
        CombinedLogger::init(vec![TermLogger::new(
            LevelFilter::from_str(configuration.minimum_log_level.as_str()).unwrap(),
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )])
        .unwrap();

        info!("Starting Cacheus server...");

        let connector = match configuration.https {
            true => HttpsConnector::new(),
            false => {
                let mut http = HttpConnector::new();
                http.set_nodelay(true);
                HttpsConnector::new_with_connector(http)
            },
        };

        let mut caches = HashMap::new();

        // Initialize caches from configuration
        for cache in &configuration.caches {
            cache.add_cache(&mut caches);
        }
        
        let server = Arc::new(CacheusServer {
            configuration: configuration.clone(),
            caches: caches,
            metrics: Metrics::new(),
            client: Client::builder(TokioExecutor)
                .http2_only(configuration.http2)
                // .pool_max_idle_per_host(configuration.max_idle_connections_per_host as usize)
                // .http2_max_send_buf_size(128_000_000)
                // .timer(hyper_util::rt::TokioTimer::new())
                // .pool_timer(hyper_util::rt::TokioTimer::new())
                // .pool_idle_timeout(std::time::Duration::from_secs(90))
                // .http2_keep_alive_interval(Some(Duration::from_secs(300)))
                // .retry_canceled_requests(false)
                .set_host(false)
                .build(connector),
        });

        let service = async {
            let service_address = SocketAddr::from(([0, 0, 0, 0], server.configuration.listening_port));
            info!(
                "Service listening on http://{}, http2:{}",
                service_address, configuration.http2
            );

            let listener = TcpListener::bind(service_address).await.unwrap();

            // We start a loop to continuously accept incoming connections
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                // Use an adapter to access something implementing `tokio::io` traits as if they implement
                // `hyper::rt` IO traits.
                let io = TokioIo::new(stream);
                let server = server.clone();
                if configuration.http2 {
                    tokio::task::spawn(async move {
                        trace!("Listening for http2 connections...");
                        let server_for_metrics = server.clone();
                        if let Err(err) = http2::Builder::new(TokioExecutor)
                            .serve_connection(io, service_fn(move |req| CacheusServer::call_async(server.clone(), req)))
                            .await
                        {
                            server_for_metrics.metrics.connection_reset.inc();
                            warn!("Error serving connection: {:?}", err);
                        }
                    });
                } else {
                    tokio::task::spawn(async move {
                        trace!("Listening for http1 connections...");
                        let server_for_metrics = server.clone();
                        if let Err(err) = http1::Builder::new()
                            .serve_connection(io, service_fn(move |req| CacheusServer::call_async(server.clone(), req)))
                            .await
                        {
                            server_for_metrics.metrics.connection_reset.inc();
                            warn!("Error serving connection: {:?}", err);
                        }
                    });
                }
            }
        };

        let prometheus = async {
            let prom_address: SocketAddr = ([0, 0, 0, 0], server.configuration.prometheus_port).into();
            info!("Prometheus listening on http://{}", prom_address);
            let listener = TcpListener::bind(prom_address).await.unwrap();

            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let io = TokioIo::new(stream);
                let server = server.clone();
                tokio::task::spawn(async move {
                    //let server = server.clone();
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(move |req| CacheusServer::prometheus(server.clone(), req)))
                        .await
                    {
                        warn!("Error serving prom connection: {:?}", err);
                    }
                });
            }
        };

        let healthcheck = async {
            let health_address: SocketAddr = ([0, 0, 0, 0], server.configuration.healthcheck_port).into();
            info!("Healthcheck listening on http://{}", health_address);
            let listener = TcpListener::bind(health_address).await.unwrap();

            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let io = TokioIo::new(stream);
                tokio::task::spawn(async move {
                    //let server = server.clone();
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(|req| CacheusServer::healthcheck(req)))
                        .await
                    {
                        warn!("Error serving healthcheck connection: {:?}", err);
                    }
                });
            }
        };

        join!(service, prometheus, healthcheck);

        Ok(())
    }

    async fn healthcheck(_req: Request<hyper::body::Incoming>) -> Result<Response<BufferedBody>, hyper::Error>
    {
        Ok(Response::new(BufferedBody::from_bytes(b"Healthy")))
    }

    async fn prometheus(
        server: Arc<CacheusServer>, _: Request<hyper::body::Incoming>,
    ) -> Result<Response<BufferedBody>, hyper::Error>
    {
        Ok(Response::new(BufferedBody::from_bytes(&server.metrics.encode())))
    }

    async fn call_async(
        service: Arc<CacheusServer>, request: Request<Incoming>,
    ) -> Result<Response<BufferedBody>, CacheusError>
    {
        trace!("Request received");

        let timestamp = std::time::Instant::now();

        let (response, status) = CacheusServer::call_internal_async(service.clone(), request).await;

        let elapsed = timestamp.elapsed();
        let status_str = status.to_string();
        service.metrics.request_duration.with_label_values(&[&status_str]).observe(elapsed.as_secs_f64());
        //service.metrics.cache_entries.set(service.cache.len() as f64);
        match response {
            Ok(ok) => {
                service.metrics.requests.with_label_values(&[&status_str, ok.status().as_str()]).inc();
                Ok(ok)
            }
            Err(e) => {
                service.metrics.requests.with_label_values(&[&status_str, "error"]).inc();
                Err(e)
            }
        }
    }

    async fn call_internal_async(service: Arc<CacheusServer>, request: Request<Incoming>,) -> (Result<Response<BufferedBody>, CacheusError>, Status)
    {
        // Request buffering
        let (parts, body) = request.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        let mut buffered_request = Request::from_parts(parts, buffered_body);

        let mut buffered_response: Option<Response<BufferedBody>> = None;

        // Evaluate middlewares
        for on_request in &service.configuration.on_request {
            match on_request.evaluate(service.clone(), &mut buffered_request).await {
                Some(response) => {
                    // If the middleware returns a response, we return it
                    buffered_response = Some(response.as_ref().clone());
                    break; // Stop processing further middlewares
                },
                None => {
                    // Continue processing other middlewares
                },
            }
        }

        info!("on_request executed");

        if buffered_response.is_none() {
            return (Err(CacheusError { message: "No response generated by on_request middlewares".to_string() }), Status::Miss);
        }

        info!("has response!");

        let mut buffered_response = buffered_response.unwrap();

        for on_response in &service.configuration.on_response {
            if on_response.evaluate(service.clone(), &mut buffered_request, &mut buffered_response).await {
                // Not sure what to do here?
            }
        }

        info!("on_response executed");

        return (Ok(buffered_response), Status::Hit);
    }
}

struct CacheusError
{
    message: String,
}

impl std::fmt::Debug for CacheusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "CacheusError: {}", self.message)
    }
}

impl Display for CacheusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "CacheusError: {}", self.message)
    }
}

impl StdError for CacheusError {}
unsafe impl Send for CacheusError {}
unsafe impl Sync for CacheusError {}