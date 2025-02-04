#[macro_use]
extern crate log;

mod buffered_body;
mod caches;
mod collections;
pub mod config;
mod executor;
mod metrics;
mod status;

use std::hash::Hash;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use buffered_body::BufferedBody;
pub use caches::*;
pub use collections::*;
pub use config::CacheusConfiguration;
use executor::TokioExecutor;
use futures::join;
use gxhash::GxHasher;
use hyper::body::Incoming;
use hyper::http::Uri;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use log::LevelFilter;
use log::Level::*;
use metrics::Metrics;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use status::Status;
use tokio::net::TcpListener;
use std::sync::atomic::Ordering;

pub struct CacheusServer
{
    configuration: CacheusConfiguration,
    cache: ShardedCache<u128, Response<BufferedBody>>,
    metrics: Metrics,
    client: Client<HttpsConnector<HttpConnector>, BufferedBody>,
}

impl CacheusServer
{
    pub async fn start_from_config_str(config_str: &str)
    {
        let configuration: CacheusConfiguration =
            serde_yaml::from_str::<CacheusConfiguration>(config_str).expect("Could not parse configuration file");
        CacheusServer::start(configuration).await.unwrap();
    }

    pub async fn start_from_config_file(config_file: &str)
    {
        let contents = std::fs::read_to_string(config_file).expect("Could not find configuration file");
        let configuration: CacheusConfiguration =
            serde_yaml::from_str::<CacheusConfiguration>(&contents).expect("Could not parse configuration file");
        CacheusServer::start(configuration).await.unwrap();
    }

    pub async fn start(configuration: CacheusConfiguration) -> Result<(), std::io::Error>
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
        
        let server = Arc::new(CacheusServer {
            configuration: configuration.clone(),
            cache: ShardedCache::<u128, Response<BufferedBody>>::new(
                configuration.in_memory_shards as usize,
                configuration.cache_probatory_size,
                configuration.cache_resident_size,
                Duration::from_secs(configuration.cache_ttl_seconds as u64),
                lru::ExpirationType::Absolute,
            ),
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

    pub async fn healthcheck(_req: Request<hyper::body::Incoming>) -> Result<Response<BufferedBody>, hyper::Error>
    {
        Ok(Response::new(BufferedBody::from_bytes(b"Healthy")))
    }

    pub async fn prometheus(
        server: Arc<CacheusServer>, _: Request<hyper::body::Incoming>,
    ) -> Result<Response<BufferedBody>, hyper::Error>
    {
        Ok(Response::new(BufferedBody::from_bytes(&server.metrics.encode())))
    }

    pub async fn call_async(
        service: Arc<CacheusServer>, request: Request<Incoming>,
    ) -> Result<Response<BufferedBody>, hyper::Error>
    {
        trace!("Request received");

        let timestamp = std::time::Instant::now();

        let (response, status) = CacheusServer::call_internal_async(service.clone(), request).await;

        let elapsed = timestamp.elapsed();
        let status_str = status.to_string();
        service.metrics.request_duration.with_label_values(&[&status_str]).observe(elapsed.as_secs_f64());
        service.metrics.requests.with_label_values(&[&status_str]).inc();
        service.metrics.cache_entries.set(service.cache.len() as f64);

        response
    }

    pub async fn call_internal_async(
        service: Arc<CacheusServer>, request: Request<Incoming>,
    ) -> (Result<Response<BufferedBody>, hyper::Error>, Status)
    {
        let trace = Arc::new(Mutex::new(String::new()));

        if log_enabled!(Debug) {
            trace.lock().unwrap().push_str(&format!("\nReceived request to path: {:?}", request.uri()));
        }

        // If path contains any of service.configuration.exclude_path_containing, return 404
        if service.configuration.exclude_path_containing.len() > 0 {
            let lowercase_path = request.uri().path().to_lowercase();
            for path in &service.configuration.exclude_path_containing {
                if lowercase_path.contains(path) {
                    return (Ok(Response::builder().status(404).body(BufferedBody::from_bytes(b"")).unwrap()), Status::Reject);
                }
            }
        }

        let key_factory = |request: &Request<BufferedBody>| {
            // Hash request content
            let mut hasher = GxHasher::with_seed(123);

            if log_enabled!(Debug) {
                trace.lock().unwrap().push_str("\nHash: (");
            }

            // Different path/query means different key
            if service.configuration.hash_path {
                if log_enabled!(Debug) {
                    trace.lock().unwrap().push_str("path + ");
                }
                request.uri().path().hash(&mut hasher);
            }
            if service.configuration.hash_query {
                if let Some(query) = request.uri().query() {
                    if log_enabled!(Debug) {
                        trace.lock().unwrap().push_str("query + ");
                    }
                    query.hash(&mut hasher);
                }
            }
            // Sometimes, we can't rely on the request body.
            // For example, protobuf maps are serialized in a non-deterministic order.
            // https://gist.github.com/kchristidis/39c8b310fd9da43d515c4394c3cd9510
            // In this case, the caller may define a hash header to not use the body for the key.
            match request.headers().get("x-request-id") {
                // If the request has a request id header, use it as the key
                Some(value) => {
                    if log_enabled!(Debug) {
                        trace.lock().unwrap().push_str("x-request-id + ");
                    }
                    value.as_bytes().hash(&mut hasher);
                },
                // Otherwise hash the request body
                None => {
                    if service.configuration.hash_body {
                        if log_enabled!(Debug) {
                            trace.lock().unwrap().push_str("body + ");
                        }
                        request.body().hash(&mut hasher);
                    }
                }
            }
            let hash = hasher.finish_u128();

            if log_enabled!(Debug) {
                // Remove last 3
                trace.lock().unwrap().pop();
                trace.lock().unwrap().pop();
                trace.lock().unwrap().pop();
                trace.lock().unwrap().push_str(&format!("): {:x}", hash));
            }

            return hash;
        };

        let status: Arc<tokio::sync::Mutex<Status>> = Arc::new(tokio::sync::Mutex::new(Status::Hit));

        let value_factory = |request: Request<BufferedBody>| async {

            trace!("Cache miss");
            let mut status = status.lock().await;
            *status = Status::Miss;

            let target_host = match request.headers().get("x-target-host") {
                Some(value) => value.to_str().unwrap().to_string(),
                None => service.configuration.default_target_host.clone(),
            };

            if target_host.is_empty() {
                panic!("Missing X-Target-Host header! Can't forward the request.");
            }

            let target_uri = Uri::builder()
                .scheme("https")
                .authority(target_host.clone())
                .path_and_query(request.uri().path_and_query().unwrap().clone())
                .build()
                .expect("Failed to build target URI");

            let mut should_bypass = false; 
            if service.configuration.bypass_path_containing.len() > 0 {
                let lowercase_path = request.uri().path().to_lowercase();
                for path in &service.configuration.bypass_path_containing {
                    if lowercase_path.contains(path) {
                        *status = Status::Bypass;
                        should_bypass = true;
                        break;
                    }
                }
            } 

            if log_enabled!(Debug) {
                trace.lock().unwrap().push_str(&format!("\nCache miss! Forwarding request to: {}", target_uri));
            }

            // Copy path and query
            let mut forwarded_req = Request::builder()
                .method(request.method())
                .uri(target_uri)
                .version(request.version());

            // Copy headers
            let headers = forwarded_req.headers_mut().expect("Failed to get headers");
            // Add host header
            headers.extend(request.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
            headers.insert("host", target_host.parse().unwrap());
            // Remove accept-encoding header, as we don't want to handle compressed responses
            headers.remove("accept-encoding");

            if log_enabled!(Debug) {
                let mut trace = trace.lock().unwrap();
                trace.push_str("\nForwarded headers:");
                for (k, v) in headers.iter() {
                    trace.push_str(&format!("\n - {}: {}", k, v.to_str().unwrap()));
                }
            }

            let body = request.into_body();

            trace!("Buffering request...");

            // Copy body
            let forwarded_req = forwarded_req.body(body).expect("Failed building request");

            trace!("Forwarding request");

            // Await the response...
            let response: Response<Incoming> = service
                .client
                .request(forwarded_req)
                .await
                .expect("Failed to send request");

            // Buffer response body so that we can cache it and return it
            let (mut parts, body) = response.into_parts();
            let mut buffered_response_body = BufferedBody::collect_buffered(body).await.unwrap();

            // Replace strings in response, but only if content type is utf8 text
            if let Some(content_type) = parts.headers.get("content-type") {
                if content_type.to_str().unwrap().contains("json") {
                    let content_length = buffered_response_body.replace_strings(&service.configuration.response_replacement_strings);
                    // Response length may have changed, so we need to update the content-length header
                    parts.headers.insert("content-length", content_length.to_string().parse().unwrap());
                }
            }

            let status = parts.status;
            
            if log_enabled!(Debug) {
                let mut trace = trace.lock().unwrap();
                trace.push_str(&format!("\nReceived response status {} and headers:", status));
                for (k, v) in parts.headers.iter() {
                    trace.push_str(&format!("\n - {}: {}", k, v.to_str().unwrap()));
                }
            }

            let cache_response: bool = match status.as_u16() {
                100..=399 => !should_bypass,
                _ => false,
            };

            Ok((Response::from_parts(parts, buffered_response_body), cache_response))
        };

        let (parts, body) = request.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        let request = Request::from_parts(parts, buffered_body);

        let result: Result<Arc<Response<BufferedBody>>, hyper::Error> = service
            .cache
            .get_or_add_from_item2(request, key_factory, value_factory)
            .await;

        let status: Status = status.lock().await.clone();

        // Log trace
        if log_enabled!(Debug) {
            let mut trace = trace.lock().unwrap();
            trace.push_str(format!("\nStatus: {}", status).as_str());
            debug!("Debug trace: {}", trace);
        }

        match result {
            Ok(response) => {
                let response = response.as_ref();
                let response: Response<BufferedBody> = response.clone();
                trace!("Received response from target with status: {:?}", response);
                (Ok(response), status)
            }
            Err(e) => (Err(e), status),
        }
    }
}
