use std::sync::Arc;
use serde::{Serialize, Deserialize};
use crate::buffered_body::BufferedBody;
use crate::CacheusServer;
use hyper::{Request, Response};

/// Response middleware configuration and logic
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum OnResponse {
    StoreCache { store_cache: StoreCache },
    AddResponseHeader { add_response_header: AddResponseHeader },
}

impl OnResponse {
    /// Evaluate the response middleware
    pub async fn evaluate(&self, service: Arc<CacheusServer>, request: &mut Request<BufferedBody>, response: &mut Response<BufferedBody>) -> bool {
        match self {
            OnResponse::StoreCache { store_cache } => {
                // Issue: computing hash twice (could be optimized later)
                // Issue: may compute hash differently (unless cache key policy is defined in the cache config)
                if let Some((cache_config, cache)) = service.caches.get(&store_cache.cache_name) {
                    let key = cache_config.create_key(request);
                    cache.try_add_arc_locked(key, response.clone());
                }
                true
            },
            OnResponse::AddResponseHeader { add_response_header: _ } => {
                // TODO: Implement header addition logic
                true
            },
        }
    }
}

/// Configuration for storing responses in cache
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoreCache {
    pub cache_name: String,
    // TODO: Add when condition
    // pub when: OnRequestCondition,
}

/// Configuration for adding response headers
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AddResponseHeader {
    pub name: String,
    pub value: String,
    // TODO: Add when condition
    // pub when: OnRequestCondition,
}
