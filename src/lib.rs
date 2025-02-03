#[macro_use]
extern crate log;

mod buffered_body;
mod caches;
mod collections;
pub mod config;
mod executor;
mod metrics;

use std::hash::Hash;
use std::net::SocketAddr;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
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
use metrics::Metrics;
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
        info!("Reading configuration from file: {}", config_file);
        let contents = std::fs::read_to_string(config_file).expect("Could not find configuration file");
        let configuration: CacheusConfiguration =
            serde_yaml::from_str::<CacheusConfiguration>(&contents).expect("Could not parse configuration file");
        CacheusServer::start(configuration).await.unwrap();
    }

    pub async fn start(configuration: CacheusConfiguration) -> Result<(), std::io::Error>
    {
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
                        debug!("Listening for http2 connections...");
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
                        debug!("Listening for http1 connections...");
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
        debug!("Request received");

        let timestamp = std::time::Instant::now();
        service.metrics.cache_calls.inc();

        let key_factory = |request: &Request<BufferedBody>| {
            // Hash request content
            let mut hasher = GxHasher::with_seed(123);
            // Different path/query means different key
            request.uri().path().hash(&mut hasher);
            request.uri().query().hash(&mut hasher);
            // Sometimes, we can't rely on the request body.
            // For example, protobuf maps are serialized in a non-deterministic order.
            // https://gist.github.com/kchristidis/39c8b310fd9da43d515c4394c3cd9510
            // In this case, the caller may define a hash header to not use the body for the key.
            match request.headers().get("x-request-id") {
                // If the request has a request id header, use it as the key
                Some(value) => value.as_bytes().hash(&mut hasher),
                // Otherwise hash the request body
                None => {
                    request.body().hash(&mut hasher);
                }
            }
            hasher.finish_u128()
        };

        let cached: AtomicBool = true.into();

        let value_factory = |request: Request<BufferedBody>| async {

            cached.store(false, Ordering::Relaxed);

            debug!("Cache miss");
            service.metrics.cache_misses.inc();

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

            info!("Headers send to {}", target_uri);

            // Copy path and query
            let mut forwarded_req = Request::builder()
                .method(request.method())
                .uri(target_uri)
                .version(request.version());

            // Copy headers
            let headers = forwarded_req.headers_mut().expect("Failed to get headers");
            // Add host header
            headers.extend(request.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
            headers.insert("Host", target_host.parse().unwrap());

            // Log all headers
            for (name, value) in request.headers() {
                info!("- {}: {}", name, value.to_str().unwrap());
            }

            let body = request.into_body();

            debug!("Buffering request...");

            // Copy body
            let forwarded_req = forwarded_req.body(body).expect("Failed building request");

            debug!("Forwarding request");

            // Await the response...
            let response: Response<Incoming> = service
                .client
                .request(forwarded_req)
                .await
                .expect("Failed to send request");

            // Buffer response body so that we can cache it and return it
            let (parts, body) = response.into_parts();
            let buffered_response_body = BufferedBody::collect_buffered(body).await.unwrap();

            let status = parts.status;
            debug!("Received response from target with status: {:?}", status);

            Ok((Response::from_parts(parts, buffered_response_body), status.is_success() /* only cache if status is successful */))
        };

        let (parts, body) = request.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        let request = Request::from_parts(parts, buffered_body);

        let result: Result<Arc<Response<BufferedBody>>, hyper::Error> = service
            .cache
            .get_or_add_from_item2(request, key_factory, value_factory)
            .await;

        let response = match result {
            Ok(response) => {
                let response = response.as_ref();
                let response: Response<BufferedBody> = response.clone();
                debug!("Received response from target with status: {:?}", response);
                Ok(response)
            }
            Err(e) => Err(e),
        };

        let elapsed = timestamp.elapsed();
        let cached_str = if cached.load(Ordering::Relaxed) { &["true"] } else { &["false"] };
        service.metrics.request_duration.with_label_values(cached_str).observe(elapsed.as_secs_f64());

        response
    }
}
