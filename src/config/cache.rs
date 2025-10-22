use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;
use gxhash::GxHasher;
use serde::{Serialize, Deserialize};
use serde_inline_default::serde_inline_default;
use crate::buffered_body::BufferedBody;
// use crate::{lru, ShardedCache};
use foyer::{*};
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
    pub fn add_cache(&self, caches: &mut HashMap<String, (CacheConfig, Cache<u128, Response<BufferedBody>>)>) {
    let hybrid: HybridCache<u64, String> = HybridCacheBuilder::new()
        .with_name("my-hybrid-cache")
        .with_policy(HybridCachePolicy::WriteOnEviction)
        .memory(1024)
        .with_shards(4)
        .with_eviction_config(LruConfig {
            high_priority_pool_ratio: 0.1,
        })
        .with_hash_builder(BuildHasherDefault::default())
        .with_weighter(|_key, value: &String| value.len())
        .with_filter(|a, x| true)
        .storage()
        .with_io_engine(io_engine)
        .with_engine_config(
            BlockEngineBuilder::new(device)
                .with_block_size(16 * 1024 * 1024)
                .with_indexer_shards(64)
                .with_recover_concurrency(8)
                .with_flushers(2)
                .with_reclaimers(2)
                .with_buffer_pool_size(256 * 1024 * 1024)
                .with_clean_block_threshold(4)
                .with_eviction_pickers(vec![Box::<FifoPicker>::default()])
                .with_admission_filter(StorageFilter::new())
                .with_reinsertion_filter(StorageFilter::new().with_condition(RejectAll))
                .with_tombstone_log(false),
        )
        .with_recover_mode(RecoverMode::Quiet)
        .with_compression(foyer::Compression::Lz4)
        .with_runtime_options(RuntimeOptions::Separated {
            read_runtime_options: TokioRuntimeOptions {
                worker_threads: 4,
                max_blocking_threads: 8,
            },
            write_runtime_options: TokioRuntimeOptions {
                worker_threads: 4,
                max_blocking_threads: 8,
            },
        })
        .build();


        // match self {
        //     CacheConfig::InMemory { in_memory } => {
        //         let cache = CacheBuilder::new(100)
        //             .with_shards(in_memory.shards)
        //             .with_eviction_config(LruConfig {
        //     high_priority_pool_ratio: 0.1,
        // })
        //             in_memory.shards,
        //             in_memory.probatory_size,
        //             in_memory.resident_size,
        //             Duration::from_secs(in_memory.ttl_seconds),
        //             lru::ExpirationType::Absolute,
        //         );
        //         caches.insert(in_memory.name.clone(), (self.clone(), cache));
        //     },
        // }
    }
}
