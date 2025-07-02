use std::sync::Arc;
use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use crate::CacheusServer;
use hyper::{Request, Response, Uri};
use hyper::body::Incoming;
use super::conditions::OnRequestCondition;

/// Request middleware configuration and logic
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnRequest {
    BlockRequest { block_request: BlockRequest },
    LookupCache { lookup_cache: LookupCache },
    ForwardRequest { forward_request: ForwardRequest },
}

impl OnRequest {
    /// Evaluate the request middleware and potentially return a response
    pub async fn evaluate(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        info!("evaluating request condition: {:?}", self);
        match self {
            OnRequest::BlockRequest { block_request } => {
                if block_request.when.as_ref().is_none_or(|w| w.evaluate(request)) {
                    return Some(Arc::new(Response::builder().status(403).body(BufferedBody::from_bytes(b"")).unwrap()))
                }
                None
            },
            OnRequest::LookupCache { lookup_cache } => {
                if let Some((cache_config, cache)) = service.caches.get(&lookup_cache.cache_name) {
                    let key = cache_config.create_key(request);
                    return match cache.try_get_locked(&key) {
                        Some(value) => {
                            info!("Cache hit for key: {}", key);
                            Some(value)
                        },
                        None => None
                    };
                }
                None
            },
            OnRequest::ForwardRequest { forward_request } => {
                info!("forwarding request to target host: {}", forward_request.target_host);

                let target_host = match request.headers().get("x-target-host") {
                    Some(value) => value.to_str().unwrap().to_string(),
                    None => forward_request.target_host.clone(),
                };

                if target_host.is_empty() {
                    panic!("Missing X-Target-Host header! Can't forward the request.");
                }

                let target_uri = Uri::builder()
                    .scheme("http")
                    .authority(target_host.clone())
                    .path_and_query(request.uri().path_and_query().unwrap().clone())
                    .build()
                    .expect("Failed to build target URI");

                // Copy path and query
                let mut forwarded_req = Request::builder()
                    .method(request.method())
                    .uri(target_uri)
                    .version(request.version())
                    .body(request.clone().into_body()).unwrap();

                // Copy headers
                let headers = forwarded_req.headers_mut();
                // Add host header
                headers.extend(request.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
                headers.insert("host", target_host.parse().unwrap());
                // Remove accept-encoding header, as we don't want to handle compressed responses
                headers.remove("accept-encoding");

                info!("Forwarding request");

                // Await the response...
                let response: Response<Incoming> = service
                    .client
                    .request(forwarded_req)
                    .await
                    .expect("Failed to send request");

                // Buffer response body so that we can cache it and return it
                let (parts, body) = response.into_parts();
                let buffered_response_body = BufferedBody::collect_buffered(body).await.unwrap();

                return Some(Arc::new(Response::from_parts(parts, buffered_response_body)));
            },
        }
    }
}

/// Configuration for blocking requests
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockRequest {
    #[serde(default)]
    pub when: Option<OnRequestCondition>,
}

/// Configuration for cache lookup
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LookupCache {
    pub cache_name: String,
    #[serde(default)]
    pub when: Option<OnRequestCondition>,
}

/// Configuration for request forwarding
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ForwardRequest {
    pub target_host: String,
    #[serde(default)]
    pub when: Option<OnRequestCondition>,
}
