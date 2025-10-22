use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;
use gxhash::GxHasher;
use serde::{Serialize, Deserialize};
use serde_inline_default::serde_inline_default;
use crate::buffered_body::BufferedBody;
use crate::{lru, ShardedCache};
use hyper::{Request, Response};

/// Cache configuration types
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum CacheConfig {
    InMemory {
        in_memory: MemoryCache,
    },
}

/// In-memory cache configuration
#[serde_inline_default]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryCache {
    pub name: String,

    #[serde_inline_default(8)]
    pub shards: usize,

    #[serde_inline_default(0)]
    pub probatory_size: usize,

    #[serde_inline_default(100_000)]
    pub resident_size: usize,

    #[serde_inline_default(3600)]
    pub ttl_seconds: u64,

    #[serde_inline_default(true)]
    pub hash_path: bool,

    #[serde_inline_default(true)]
    pub hash_query: bool,

    #[serde_inline_default(true)]
    pub hash_body: bool,
}

impl CacheConfig {
    /// Create a cache key from a request
    pub fn create_key(&self, request: &Request<BufferedBody>) -> u128 {
        match self {
            CacheConfig::InMemory { in_memory } => {
                let mut hasher = GxHasher::with_seed(123);
                // Different path/query means different key
                if in_memory.hash_path {
                    request.uri().path().hash(&mut hasher);
                }
                if in_memory.hash_query {
                    if let Some(query) = request.uri().query() {
                        query.hash(&mut hasher);
                    }
                }
                if in_memory.hash_body {
                    request.body().hash(&mut hasher);
                }
                hasher.finish_u128()
            },
        }
    }

    /// Add cache to the provided cache map
    pub fn add_cache(&self, caches: &mut HashMap<String, (CacheConfig, ShardedCache<u128, CachedResponse>)>) {
        match self {
            CacheConfig::InMemory { in_memory } => {
                let cache = ShardedCache::<u128, CachedResponse>::new(
                    in_memory.shards,
                    in_memory.probatory_size,
                    in_memory.resident_size,
                    Duration::from_secs(in_memory.ttl_seconds),
                    lru::ExpirationType::Absolute,
                );
                caches.insert(in_memory.name.clone(), (self.clone(), cache));
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedResponse {
    #[serde(with = "http_serde_ext::response")]
    pub response: Response<BufferedBody>,
}