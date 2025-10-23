use std::{str::FromStr, sync::Arc};

use hyper::{header::{HeaderName, HeaderValue}, Request, Response, Uri};
use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use crate::{buffered_body::BufferedBody, config::conditions::Condition, status::Status, CacheusServer, CallContext};

trait Middleware {
    async fn on_request(&self, context: &mut CallContext, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)>;
    async fn on_response(&self, context: &mut CallContext, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MiddlewareEnum {
    Block { block_request: Block },
    Cache { use_cache: Cache }, // TODO: Support HTTP cache headers, validation, etc.
    Forward { forward_request: Forward }, // TODO: Support multiple targets, DNS load balancing, redirects, etc.
    AddHeader { add_response_header: AddHeader }, // TODO: If before forward: add request header, else add response header? Or separate types?
    ReplaceResponseBody { replace_response_body: ReplaceResponseBody },
    Log { log: Log },
    // TODO: RateLimit, Compress, 
}

impl MiddlewareEnum {
    pub async fn on_request(&self, context: &mut CallContext, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        debug!("Processing middleware request: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_request(context, service, request).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_request(context, service, request).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_request(context, service, request).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_request(context, service, request).await,
            MiddlewareEnum::ReplaceResponseBody { replace_response_body } => replace_response_body.on_request(context, service, request).await,
            MiddlewareEnum::Log { log } => log.on_request(context, service, request).await,
        }
    }

    pub async fn on_response(&self, context: &mut CallContext, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        debug!("Processing middleware response: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_response(context, service, request, response).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_response(context, service, request, response).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_response(context, service, request, response).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_response(context, service, request, response).await,
            MiddlewareEnum::ReplaceResponseBody { replace_response_body } => replace_response_body.on_response(context, service, request, response).await,
            MiddlewareEnum::Log { log } => log.on_response(context, service, request, response).await,
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
    async fn on_request(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        if !self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return None;
        }
        return Some((Arc::new(Response::builder().status(self.status_code).body(BufferedBody::from_bytes(b"")).unwrap()), Status::Block));
    }

    async fn on_response(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, _request: &Request<BufferedBody>, _response: &mut Response<BufferedBody>) {

    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cache {
    pub cache_name: String,
    #[serde(default)]
    pub when: Option<Condition>,
}

impl Middleware for Cache {
    async fn on_request(&self, _context: &mut CallContext, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        if !self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return None;
        }
        if let Some((cache_config, cache_instance)) = service.caches.get(&self.cache_name) {
            let key = cache_config.create_key(&request);
            if let Some(cached_response) = cache_instance.get(&key) {
                _context.variables.insert("$cache_status".to_string(), "hit".to_string());
                return Some((Arc::new(cached_response.value().clone()), Status::Hit)); // Clone ? Why not Arc ?
            }
        }
        _context.variables.insert("$cache_status".to_string(), "miss".to_string());
        return None;
    }

    async fn on_response(&self, _context: &mut CallContext, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        if let Some((cache_config, cache_instance)) = service.caches.get(&self.cache_name) {
            
            let key = cache_config.create_key(&request);
            cache_instance.insert(key, response.clone());
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Forward {
    pub target_host: String,
    #[serde(default)]
    pub scheme: Option<String>,
}

impl Middleware for Forward {
    async fn on_request(&self, context: &mut CallContext, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
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

        let res = service.client.request(forwarded_req).await.expect("Failed to send request");
        let (parts, body) = res.into_parts();
        let buffered_body = BufferedBody::collect_buffered(body).await.unwrap();
        return Some((Arc::new(Response::from_parts(parts, buffered_body)), Status::Miss));
    }

    async fn on_response(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, _request: &Request<BufferedBody>, _response: &mut Response<BufferedBody>) {

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
    async fn on_request(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, _request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        None
    }

    async fn on_response(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        // Replace variables in header value, if any
        let value = _context.variables.iter().fold(self.value.clone(), |acc, (k, v)| acc.replace(k.as_str(), v.as_str()));
        // Insert the header
        response.headers_mut().insert(
            HeaderName::from_str(self.name.as_str()).unwrap(), // Clearly not optimal
            HeaderValue::from_str(value.as_str()).unwrap());
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
    async fn on_request(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, _request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        None
    }

    async fn on_response(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        if let Some(content_type) = response.headers().get("content-type") {
            if content_type.to_str().unwrap().contains("json") {
                let content_length = response.body_mut().replace_strings(&self.find, &self.replacement);
                // Response length may have changed, so we need to update the content-length header
                response.headers_mut().insert("content-length", content_length.to_string().parse().unwrap());
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
    async fn on_request(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, _request: &mut Request<BufferedBody>) -> Option<(Arc<Response<BufferedBody>>, Status)> {
        None
    }

    async fn on_response(&self, _context: &mut CallContext, _service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if !self.when.as_ref().is_none_or(|w| w.evaluate_with_response(&request, &response)) {
            return;
        }
        // Replace variables in header value, if any
        let value = _context.variables.iter().fold(self.value.clone(), |acc, (k, v)| acc.replace(k.as_str(), v.as_str()));
        log::log!(log::Level::from_str(self.level.as_str()).unwrap_or(log::Level::Info), "{}", value);
    }
}