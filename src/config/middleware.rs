use std::{str::FromStr, sync::Arc};

use hyper::{header::{HeaderName, HeaderValue}, Request, Response, Uri};
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use crate::{CallContext, buffered_body::BufferedBody, config::{CacheConfig, cache::CachedResponse, conditions::Condition}, executor::TokioExecutor};

trait Middleware {
    async fn init(&mut self) {}
    async fn on_request(&self, _context: &mut CallContext, _request: &mut Request<BufferedBody>) -> Option<Response<BufferedBody>> { None }
    async fn on_response(&self, _context: &mut CallContext, _request: &Request<BufferedBody>, _response: &mut Option<Response<BufferedBody>>) {}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MiddlewareEnum {
    Block { block_request: Block },
    Cache { use_cache: CacheMiddleware }, // TODO: Support HTTP cache headers, validation, etc.
    Forward { forward_request: Forward }, // TODO: Support multiple targets, DNS load balancing, redirects, etc.
    AddHeader { add_response_header: AddHeader }, // TODO: If before forward: add request header, else add response header? Or separate types?
    ReplaceResponseBody { replace_response_body: ReplaceResponseBody },
    Log { log: Log },
    // TODO: RateLimit, Compress, 
}

impl MiddlewareEnum {
    pub async fn on_request(&self, context: &mut CallContext, request: &mut Request<BufferedBody>) -> Option<Response<BufferedBody>> {
        debug!("Processing middleware request: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_request(context, request).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_request(context, request).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_request(context, request).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_request(context, request).await,
            MiddlewareEnum::ReplaceResponseBody { replace_response_body } => replace_response_body.on_request(context, request).await,
            MiddlewareEnum::Log { log } => log.on_request(context, request).await,
        }
    }

    pub async fn on_response(&self, context: &mut CallContext, request: &Request<BufferedBody>, response: &mut Option<Response<BufferedBody>>) {
        debug!("Processing middleware response: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_response(context, request, response).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_response(context, request, response).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_response(context, request, response).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_response(context, request, response).await,
            MiddlewareEnum::ReplaceResponseBody { replace_response_body } => replace_response_body.on_response(context, request, response).await,
            MiddlewareEnum::Log { log } => log.on_response(context, request, response).await,
        }
    }
}

#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block {
    #[serde_inline_default(403)]
    pub status_code: u16,
    #[serde(default)]
    pub when: Option<Condition>,
}

impl Middleware for Block {
    async fn on_request(&self, _context: &mut CallContext, request: &mut Request<BufferedBody>) -> Option<Response<BufferedBody>> {
        if !self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return None;
        }
        return Some(Response::builder().status(self.status_code).body(BufferedBody::from_body(b"Blocked by cacheus")).unwrap());
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheMiddleware {
    pub cache_name: String,
    #[serde(default)]
    pub when: Option<Condition>,
}

// - Stale
// [Cache::on_request] lookup: expired -> [Forward::on_request] failed -> [Forward::on_response] -> [Cache::on_response] no response: lookup: expired: mark stale
// - Miss
// [Cache::on_request] lookup: not found -> [Forward::on_request] -> [Forward::on_response] -> [Cache::on_response] response: insert
// [Cache::on_request] lookup: error -> [Forward::on_request] -> [Forward::on_response] -> [Cache::on_response] response: insert
// - Hit
// [Cache::on_request] lookup: found -> [Cache::on_response] response + hit context: no insert
impl Middleware for CacheMiddleware {
    async fn on_request(&self, context: &mut CallContext, request: &mut Request<BufferedBody>) -> Option<Response<BufferedBody>> {
        if !self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return None;
        }
        if let Some((cache_config, cache_instance)) = context.caches.get(&self.cache_name) {
            let key = cache_config.create_key(&request);
            match cache_instance.get(&key).await {
                Err(e) => {
                    log::error!("Cache lookup error for key {:?}: {:?}", key, e);
                    // Error during lookup, treat as miss
                    context.variables.insert("$cache_status".to_string(), "miss".to_string());
                },
                Ok(cached_response) => match cached_response {
                    None => {
                        // True cache miss
                        context.variables.insert("$cache_status".to_string(), "miss".to_string());
                    },
                    Some(cached_response) => {
                        let current_epoch: u64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                        let ttl_seconds = match cache_config {
                            CacheConfig::InMemory { in_memory } => in_memory.ttl_seconds,
                            CacheConfig::Hybrid { hybrid } => hybrid.ttl_seconds
                        };
                        if current_epoch - cached_response.value().insertion_epoch > ttl_seconds {
                            // Stale entry! We keep it in case we need it later
                            context.stale_response.replace(cached_response.value().response.clone());
                            // Stale is considered a miss in the context of the a fresh lookup
                            context.variables.insert("$cache_status".to_string(), "miss".to_string());
                        }
                        else
                        {
                            // Cache hit!
                            context.variables.insert("$cache_status".to_string(), "hit".to_string());
                            return Some(cached_response.value().response.clone());
                        }
                    }
                }
            }
        }
        else
        {
            log::error!("No cache named '{}' found. Verify configuration.", self.cache_name);
            // Cache not found, treat as miss
            context.variables.insert("$cache_status".to_string(), "miss".to_string());
        }
        return None;
    }

    async fn on_response(&self, context: &mut CallContext, request: &Request<BufferedBody>, response: &mut Option<Response<BufferedBody>>) {
        // Here we have a problem: imagine we only want to cache 200 OK responses, so we add when: status_code_is: 200. 
        // If the response is 404, we may want to return a stale instead of a miss. But since the when condition fails, we won't even look into the cache.
        // To handle such case, we handle the stale logic before the when condition.
        let has_response = response.is_some();
        let when_applies = self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response));
        if (!has_response || !when_applies) && context.stale_response.is_some() {
            context.variables.insert("$cache_status".to_string(), "stale".to_string());
            response.replace(context.stale_response.take().unwrap());
        }
        if !when_applies {
            return;
        }
        if has_response
        {
            // Response: either miss (fetched from subsequent middleware) or hit (taken from cache)
            // In case of miss, we insert the response into the cache
            if context.variables.get("$cache_status").map_or(false, |v| v == "hit") {
                return;
            }
            if let Some((cache_config, cache_instance)) = context.caches.get(&self.cache_name) {
                let key = cache_config.create_key(&request);
                let insertion_epoch: u64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                cache_instance.insert(key, CachedResponse { insertion_epoch: insertion_epoch, response: response.clone().unwrap() });
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Forward {
    pub target_host: String,
    #[serde(default)]
    pub enforce_http2: bool,
    #[serde(default)]
    pub enforce_http: bool,
    #[serde(default)]
    pub use_nagle: bool,
    #[serde(default)]
    pub scheme: Option<String>,
    #[serde(default)]
    pub when: Option<Condition>,
    #[serde(skip)]
    pub client: Option<Arc<Client<HttpsConnector<HttpConnector>, BufferedBody>>>,
}

impl Middleware for Forward {

    async fn init(&mut self) {

        let mut http = HttpConnector::new();
        http.set_nodelay(!self.use_nagle);
        http.enforce_http(self.enforce_http);
        let connector = HttpsConnector::new_with_connector(http);

        let client = Client::builder(TokioExecutor)
            .http2_only(self.enforce_http2)
            // .pool_max_idle_per_host(configuration.max_idle_connections_per_host as usize)
            // .http2_max_send_buf_size(128_000_000)
            // .timer(hyper_util::rt::TokioTimer::new())
            // .pool_timer(hyper_util::rt::TokioTimer::new())
            // .pool_idle_timeout(std::time::Duration::from_secs(90))
            // .http2_keep_alive_interval(Some(Duration::from_secs(300)))
            // .retry_canceled_requests(false)
            .set_host(false)
            .build(connector);

        self.client = Some(Arc::new(client));
    }

    async fn on_request(&self, context: &mut CallContext, request: &mut Request<BufferedBody>) -> Option<Response<BufferedBody>> {
        if !self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return None;
        }
        let target_host = match request.headers().get("x-target-host") {
            Some(value) => value.to_str().unwrap().to_string(),
            None => self.target_host.clone(),
        };

        if target_host.is_empty() {
            panic!("Missing X-Target-Host header! Can't forward the request.");
        }

        let target_uri = Uri::builder()
            .scheme(self.scheme.clone().unwrap_or("http".to_string()).as_str()) // Suboptimal
            .authority(target_host.clone())
            .path_and_query(request.uri().path_and_query().unwrap().clone())
            .build()
            .expect("Failed to build target URI");

        context.variables.insert("$forward_target".to_string(), target_uri.to_string());

        let mut forwarded_req = Request::builder()
            .method(request.method())
            .uri(target_uri)
            .version(request.version())
            .body(request.body().clone()).unwrap();

        let headers = forwarded_req.headers_mut();
        headers.extend(request.headers().iter().map(|(k, v)| (k.clone(), v.clone())));
        headers.insert("host", target_host.parse().unwrap());
        headers.remove("accept-encoding");

        let res = self.client.as_ref().unwrap().request(forwarded_req).await.expect("Failed to send request");
        let (parts, body) = res.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        return Some(Response::from_parts(parts, buffered_body));
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub when: Option<Condition>,
}

impl Middleware for AddHeader {

    async fn on_response(&self, _context: &mut CallContext, request: &Request<BufferedBody>, response: &mut Option<Response<BufferedBody>>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        if let Some(response) = response {
            // Replace variables in header value, if any
            let value = _context.variables.iter().fold(self.value.clone(), |acc, (k, v)| acc.replace(k.as_str(), v.as_str()));
            // Insert the header
            response.headers_mut().insert(
                HeaderName::from_str(self.name.as_str()).unwrap(), // Clearly not optimal
                HeaderValue::from_str(value.as_str()).unwrap());
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReplaceResponseBody {
    pub find: String,
    pub replacement: String,
    #[serde(default)]
    pub when: Option<Condition>,
}

impl Middleware for ReplaceResponseBody {

    async fn on_response(&self, _context: &mut CallContext, request: &Request<BufferedBody>, response: &mut Option<Response<BufferedBody>>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        if let Some(response) = response {
            if let Some(content_type) = response.headers().get("content-type") {
                if content_type.to_str().unwrap().contains("json") {
                    let content_length = response.body_mut().replace_strings(&self.find, &self.replacement);
                    // Response length may have changed, so we need to update the content-length header
                    response.headers_mut().insert("content-length", content_length.to_string().parse().unwrap());
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Log {
    pub level: String,
    pub value: String,
    #[serde(default)]
    pub when: Option<Condition>,
}

impl Middleware for Log {

    async fn on_response(&self, _context: &mut CallContext, request: &Request<BufferedBody>, response: &mut Option<Response<BufferedBody>>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        // Replace variables in header value, if any
        let value = _context.variables.iter().fold(self.value.clone(), |acc, (k, v)| acc.replace(k.as_str(), v.as_str()));
        log::log!(log::Level::from_str(self.level.as_str()).unwrap_or(log::Level::Info), "{}", value);
    }
}

#[cfg(test)]
mod tests {
    use log::LevelFilter;
    use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};

    use super::*;

    #[tokio::test]
    async fn test_cache_middleware() {

        // Configure logging to print to console
        CombinedLogger::init(vec![TermLogger::new(
            LevelFilter::Debug,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )])
        .unwrap();

        // Create middleware
        let middleware = CacheMiddleware {
            cache_name: "test_cache".to_string(),
            when: None,
        };

        // Create cache
        let mut caches = std::collections::HashMap::new();
        let cache_config = CacheConfig::InMemory { in_memory: crate::config::cache::MemoryCache {
                name: "test_cache".to_string(),
                ttl_seconds: 2,
                ..Default::default()
            }
        };
        cache_config.add_cache(&mut caches).await;
        assert!(caches.contains_key("test_cache"));

        // Create call context
        let mut call_context = CallContext {
            caches: Arc::new(caches),
            ..Default::default()
        };

        // Create request
        let mut request = Request::builder().uri("/test").body(BufferedBody::default()).unwrap();
        let mut remote_response = Some(Response::builder().status(200).body(BufferedBody::from_body(b"Hello, world!")).unwrap());

        // Call middleware first, expecting a miss
        let cache_response = middleware.on_request(&mut call_context, &mut request).await;
        middleware.on_response(&mut call_context, &request, &mut remote_response).await;

        assert!(cache_response.is_none());
        assert_eq!(call_context.variables.get("$cache_status"), Some(&"miss".to_string()));

        // Call middleware second, expecting a hit
        let mut cache_response = middleware.on_request(&mut call_context, &mut request).await;
        assert!(cache_response.is_some());
        middleware.on_response(&mut call_context, &request, &mut cache_response).await;

        assert_eq!(call_context.variables.get("$cache_status"), Some(&"hit".to_string()));

        // Wait for 3s, then entry should be stale
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let cache_response = middleware.on_request(&mut call_context, &mut request).await;
        let mut stale_response = None; // Simulate failed fetch
        middleware.on_response(&mut call_context, &request, &mut stale_response).await;

        assert!(cache_response.is_none(), "Expected no response from cache since entry is stale");
        assert!(stale_response.is_some(), "Expected stale response to be returned");
        assert_eq!(call_context.variables.get("$cache_status"), Some(&"stale".to_string()));

        // Call again, but now the remote call is successful, so we should cache the new response and get a miss
        let cache_response = middleware.on_request(&mut call_context, &mut request).await;
        middleware.on_response(&mut call_context, &request, &mut remote_response).await;

        assert!(cache_response.is_none(), "Expected no response from cache since entry is stale");
        assert_eq!(call_context.variables.get("$cache_status"), Some(&"miss".to_string()));
    }
}