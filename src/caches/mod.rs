pub mod lru;
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
pub use lru::LruCache;

pub mod probatory;
pub use probatory::ProbatoryCache;

pub mod sharded;

pub use sharded::ShardedCache;

#[enum_dispatch]
trait Cache<K, V> {
    fn len(&self) -> usize;
    fn try_add(&mut self, key: K, value: V) -> bool;
    fn try_get(&mut self, key: &K) -> Option<Arc<V>>;
}

#[enum_dispatch(Cache<K, V>)]
pub enum CacheEnum<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    Lru(LruCache<K, V>),
    Probatory(ProbatoryCache<K, V>),
    Sharded(ShardedCache<K, V>),
}