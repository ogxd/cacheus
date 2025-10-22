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
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

pub use caches::*;
pub use collections::*;
pub use config::{Configuration, MiddlewareEnum};
use buffered_body::BufferedBody;
use executor::TokioExecutor;
use futures::join;
use hyper::body::{Body, Incoming};
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request, Response, Uri};
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
use crate::config::cache::CachedResponse;
use crate::config::CacheConfig;

pub struct CacheusServer
{
    configuration: Configuration,
    caches: HashMap<String, (CacheConfig, ShardedCache<u128, CachedResponse>)>,
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

        let mut http = HttpConnector::new();
        http.set_nodelay(true);
        http.enforce_http(false);
        let connector = HttpsConnector::new_with_connector(http);

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

        // Log created middlewares
        for middleware in &server.configuration.middlewares {
            info!("Loaded middleware: {:?}", middleware);
        }

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
        let mut context = CallContext {
            variables: HashMap::new(),
        };
        
        // Request buffering
        let (parts, body) = request.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        let mut buffered_request = Request::from_parts(parts, buffered_body);

        let mut response: Option<(Arc<Response<BufferedBody>>, Status)> = None;
        let mut i = 0;

        // Evaluate middlewares until one returns a response
        for middleware in &service.configuration.middlewares {
            i += 1;
            if let Some(r) = middleware.on_request(&mut context, service.clone(), &mut buffered_request).await {
                response = Some(r);
                break; // On the first middleware that returns a response, we stop processing
            }
        }

        if response.is_none() {
            return (Err(CacheusError { message: "No response generated by middlewares".to_string() }), Status::Ignore);
        }

        let response = response.unwrap();
        let status = response.1;
        let mut response = response.0.deref().clone();

        // In reverse order, apply middlewares on the response, starting from the one that produced the response
        for j in (0..i).rev() {
            service.configuration.middlewares[j].on_response(&mut context, service.clone(), &buffered_request, &mut response).await;
        }

        return (Ok(response), status);
    }
}

pub struct CallContext {
    variables: HashMap<String, String>,
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