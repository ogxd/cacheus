use crate::ProbatoryCache;
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::lru::ExpirationType;

#[allow(dead_code)]
pub struct ShardedCache<K, V>
{
    shards: Vec<Arc<Mutex<ProbatoryCache<K, V>>>>,
}

#[allow(dead_code)]
impl<K, V> ShardedCache<K, V>
where
    K: Eq + std::hash::Hash + Clone
{
    pub fn new(shards: usize, max_size: usize, expiration: Duration, expiration_type: ExpirationType) -> Self {
        Self {
            shards: (0..shards).map(|_| Arc::new(Mutex::new(ProbatoryCache::new(max_size, expiration, expiration_type)))).collect()
        }
    }

    fn get_shard(&self, key: &K) -> &Arc<Mutex<ProbatoryCache<K, V>>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        let shard = hash % self.shards.len();
        &self.shards[shard]
    }

    pub fn try_add(&mut self, key: K, value: V) -> bool {
        self.get_shard(&key).lock().unwrap().try_add(key, value)
    }

    pub fn try_get(&mut self, key: &K) -> Option<Arc<V>> {
        self.get_shard(&key).lock().unwrap().try_get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let mut lru = ShardedCache::new(1, 4, Duration::MAX, ExpirationType::Absolute);
        assert!(lru.try_get(&1).is_none());
        assert!(lru.try_add(1, "hello"));
        assert!(lru.try_get(&1).is_none(), "Key should only be in the probatory cache");
        assert!(lru.try_add(1, "hello"));
        assert!(lru.try_get(&1).is_some(), "Key should now be in the resident cache");
        assert!(!lru.try_add(1, "hello"));
    }

    #[test]
    fn trimming() {
        let mut lru = ShardedCache::new(1, 4, Duration::MAX, ExpirationType::Absolute);
        // Add every entry twice for them to enter the resident cache
        assert!(lru.try_add(1, "h"));
        assert!(lru.try_add(1, "h"));
        assert!(lru.try_add(2, "e"));
        assert!(lru.try_add(2, "e"));
        assert!(lru.try_add(3, "l"));
        assert!(lru.try_add(3, "l"));
        assert!(lru.try_add(4, "l"));
        assert!(lru.try_add(4, "l"));
        // Max size is reached, next insertion should evict oldest entry, which is 1
        assert!(lru.try_add(5, "o"));
        assert!(lru.try_add(5, "o"));
        assert!(lru.try_get(&1).is_none());
        assert!(lru.try_get(&2).is_some());
        assert!(lru.try_get(&3).is_some());
        assert!(lru.try_get(&4).is_some());
        assert!(lru.try_get(&5).is_some());
    }
}