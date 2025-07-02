pub mod lru;
use std::sync::Arc;

pub use lru::LruCache;

pub mod probatory;
pub use probatory::ProbatoryCache;

pub mod sharded;

pub use sharded::ShardedCache;

pub enum CacheEnum<K, V>
{
    Lru(LruCache<K, V>),
    Probatory(ProbatoryCache<K, V>),
    ShardedCache(ShardedCache<K, V>),
}

impl<K, V> CacheEnum<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    pub fn len(&self) -> usize
    {
        match self {
            CacheEnum::Lru(cache) => cache.len(),
            CacheEnum::Probatory(cache) => cache.len(),
            CacheEnum::ShardedCache(cache) => cache.len(),
        }
    }

    pub fn try_add_arc(&mut self, key: K, value: Arc<V>) -> bool
    {
        match self {
            CacheEnum::Lru(cache) => cache.try_add_arc(key, value),
            CacheEnum::Probatory(cache) => cache.try_add_arc(key, value),
            CacheEnum::ShardedCache(cache) => cache.try_add_arc(key, value),
        }
    }

    pub fn try_get(&mut self, key: &K) -> Option<Arc<V>>
    {
        match self {
            CacheEnum::Lru(cache) => cache.try_get(key),
            CacheEnum::Probatory(cache) => cache.try_get(key),
            CacheEnum::ShardedCache(cache) => cache.try_get(key),
        }
    }
}