use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use hyper::{Request, Response, Uri};
use hyper::body::Incoming;
use serde_inline_default::serde_inline_default;
use crate::{lru, CacheusServer, ShardedCache};
use crate::status::Status;

#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    #[serde_inline_default(3001)]
    pub listening_port: u16,
    #[serde_inline_default(8000)]
    pub prometheus_port: u16,
    #[serde_inline_default(8001)]
    pub healthcheck_port: u16,
    #[serde_inline_default(true)]
    pub https: bool,
    #[serde_inline_default(true)]
    pub http2: bool,
    #[serde_inline_default("info".to_string())]
    pub minimum_log_level: String,
    pub default_target_host: String,
    pub on_request: Vec<OnRequest>,
    pub on_response: Vec<OnResponse>,
    pub caches: Vec<Cache>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnRequest {
    BlockRequest { block_request: BlockRequest },
    LookupCache { lookup_cache: LookupCache },
    ForwardRequest { forward_request: ForwardRequest },
}

impl OnRequest {
    pub async fn evaluate(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>)  -> Option<Response<BufferedBody>> {
        match self {
            OnRequest::BlockRequest { block_request } => {
                if block_request.when.evaluate(request) {
                    return Some(Response::builder().status(403).body(BufferedBody::from_bytes(b"")).unwrap())
                }
                None
            },
            OnRequest::LookupCache { lookup_cache } => {
                // Get the cache
                // Lookup in the cache
                None
            },
            OnRequest::ForwardRequest { forward_request } => {

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
                let headers = forwarded_req.headers_mut();//.expect("Failed to get headers");
                // Add host header
                headers.extend(request.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
                headers.insert("host", target_host.parse().unwrap());
                // Remove accept-encoding header, as we don't want to handle compressed responses
                headers.remove("accept-encoding");
                
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

                let response = Response::from_parts(parts, buffered_response_body);
                
                return Some(response);
            },
            _ => None, // Ignore response conditions at this phase
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnResponse {
    StoreCache { store_cache: StoreCache },
    AddResponseHeader { add_response_header: AddResponseHeader },
}

impl OnResponse {
    pub async fn evaluate(&self, request: &mut Request<BufferedBody>, response: &mut Response<BufferedBody>)  -> bool {
        match self {
            OnResponse::StoreCache { store_cache } => {
                true
            },
            OnResponse::AddResponseHeader { add_response_header } => {
                true
            },
            _ => true, // Ignore response conditions at this phase
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockRequest {
    pub when: OnRequestCondition,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LookupCache {
    pub cache_name: String,
    pub when: OnRequestCondition,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ForwardRequest {
    pub target_host: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoreCache {
    pub cache_name: String,
    pub when: OnRequestCondition,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddResponseHeader {
    pub name: String,
    pub value: String,
    pub when: OnRequestCondition,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnRequestCondition {
    HeaderExists { header_exists: String },
    PathContains { path_contains: String },
    StatusCodeEquals { status_code: u16 },
    All { all: Vec<OnRequestCondition> },
    Any { any: Vec<OnRequestCondition> },
}

impl OnRequestCondition {
    pub fn evaluate(&self, req: &Request<BufferedBody>) -> bool {
        match self {
            OnRequestCondition::HeaderExists { header_exists } => req.headers().contains_key(header_exists),
            OnRequestCondition::PathContains { path_contains } => req.uri().path_and_query().unwrap().as_str().contains(path_contains),
            OnRequestCondition::All { all } => all.iter().all(|c| c.evaluate(req)),
            OnRequestCondition::Any { any } => any.iter().any(|c| c.evaluate(req)),
            _ => false, // Ignore response conditions at this phase
        }
    }

    // pub fn evaluate_response(&self, req: &Request<BufferedBody>, res: &Response<BufferedBody>) -> bool {
    //     match self {
    //         Condition::StatusCodeEquals { status_code } => res.status().as_u16().eq(status_code),
    //         Condition::All { all } => all.iter().all(|c| c.evaluate_response(req, res)),
    //         Condition::Any { any } => any.iter().any(|c| c.evaluate_response(req, res)),
    //         _ => false,
    //     }
    // }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Cache {
    InMemory {
        in_memory: MemoryCache,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryCache {
    pub name: String,
    pub probatory_size: usize,
    pub resident_size: usize,
    pub ttl_seconds: u64,
    pub hash_body: bool,
}

impl Cache {
    pub fn add_cache(&self, caches: &mut HashMap<String, ShardedCache<u128, Response<BufferedBody>>>) {
        match self {
            Cache::InMemory { in_memory } => {
                let cache = ShardedCache::<u128, Response<BufferedBody>>::new(
                    12usize,
                    in_memory.probatory_size,
                    in_memory.resident_size,
                    Duration::from_secs(in_memory.ttl_seconds),
                    lru::ExpirationType::Absolute,
                );
                caches.insert(in_memory.name.clone(), cache);
            },
            _ => {}, // Ignore response conditions at this phase
        }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    // #[test]
    // fn test_config_deserialization()
    // {
    //     let conf = "in_memory_shards: 42\n\
    //                 cache_resident_size: 123\n\
    //                 cache_probatory_size: 456\n\
    //                 listening_port: 789\n\
    //                 http2: false";

    //     let configuration: Configuration = serde_yaml::from_str::<Configuration>(conf).unwrap();

    //     assert_eq!(configuration.in_memory_shards, 42);
    //     assert_eq!(configuration.cache_resident_size, 123);
    //     assert_eq!(configuration.cache_probatory_size, 456);
    //     assert_eq!(configuration.listening_port, 789);
    // }
}
