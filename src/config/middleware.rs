use std::{str::FromStr, sync::Arc};

use enum_dispatch::enum_dispatch;
use hyper::{header::{HeaderName, HeaderValue}, Request, Response, Uri};
use serde::{Serialize, Deserialize};
use crate::{buffered_body::BufferedBody, config::conditions::{OnRequestCondition, OnResponseCondition}, CacheusServer};

// #[enum_dispatch]
trait Middleware {
    async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>>;
    async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
// #[enum_dispatch(Middleware)]
pub enum MiddlewareEnum {
    Block { block_request: Block },
    Cache { use_cache: Cache },
    Forward { forward_request: Forward },
    AddHeader { add_response_header: AddHeader },
}

impl MiddlewareEnum {
    pub async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        info!("Processing middleware request: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_request(service, request).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_request(service, request).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_request(service, request).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_request(service, request).await,
        }
    }

    pub async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        info!("Processing middleware response: {:?}", self);
        match self {
            MiddlewareEnum::Block { block_request } => block_request.on_response(service, request, response).await,
            MiddlewareEnum::Cache { use_cache } => use_cache.on_response(service, request, response).await,
            MiddlewareEnum::Forward { forward_request } => forward_request.on_response(service, request, response).await,
            MiddlewareEnum::AddHeader { add_response_header } => add_response_header.on_response(service, request, response).await,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block {
    #[serde(default)]
    pub when: Option<OnRequestCondition>,
}

impl Middleware for Block {
    async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        if self.when.as_ref().is_none_or(|w| w.evaluate(&request)) {
            return Some(Arc::new(Response::builder().status(403).body(BufferedBody::from_bytes(b"")).unwrap()));
        }
        return None;
    }

    async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {

    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cache {
    pub cache_name: String,
    #[serde(default)]
    pub when_request: Option<OnRequestCondition>,
    #[serde(default)]
    pub when_response: Option<OnResponseCondition>,
}

impl Middleware for Cache {
    async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        if self.when_request.as_ref().is_none_or(|w| w.evaluate(&request)) {
            if let Some((cache_config, cache_instance)) = service.caches.get(&self.cache_name) {
                let key = cache_config.create_key(&request);
                if let Some(cached_response) = cache_instance.try_get_locked(&key) {
                    return Some(Arc::new(cached_response.as_ref().clone()));
                }
            }
        }
        return None;
    }

    async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if self.when_response.as_ref().is_none_or(|w| w.evaluate(&request, &response)) {
            if let Some((cache_config, cache_instance)) = service.caches.get(&self.cache_name) {
                let key = cache_config.create_key(&request);
                cache_instance.try_add_arc_locked(key, response.clone());
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Forward {
    pub target_host: String,
}

impl Middleware for Forward {
    async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        let target_host = match request.headers().get("x-target-host") {
            Some(value) => value.to_str().unwrap().to_string(),
            None => self.target_host.clone(),
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
        return Some(Arc::new(Response::from_parts(parts, buffered_body)));
    }

    async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {

    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub when_response: Option<OnResponseCondition>,
}

impl Middleware for AddHeader {
    async fn on_request(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>) -> Option<Arc<Response<BufferedBody>>> {
        None
    }

    async fn on_response(&self, service: Arc<CacheusServer>, request: &Request<BufferedBody>, response: &mut Response<BufferedBody>) {
        if self.when_response.as_ref().is_none_or(|w| w.evaluate(&request, &response)) {
            response.headers_mut().insert(
                HeaderName::from_str(self.name.as_str()).unwrap(), // Clearly not optimal
                HeaderValue::from_str(self.value.as_str()).unwrap())
                .unwrap();
        }
    }
}