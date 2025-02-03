use std::future::Future;
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::lru::ExpirationType;
use crate::{Cache, ProbatoryCache};

#[allow(dead_code)]
pub struct ShardedCache<K, V>
{
    shards: Vec<Arc<Mutex<ProbatoryCache<K, V>>>>,
}

impl<K, V> Cache<K, V> for ShardedCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn try_add_arc(&mut self, key: K, value: Arc<V>) -> bool
    {
        self.get_shard(&key).lock().unwrap().try_add_arc(key, value)
    }

    fn try_get(&mut self, key: &K) -> Option<Arc<V>>
    {
        self.get_shard(&key).lock().unwrap().try_get(key)
    }
}

#[allow(dead_code)]
impl<K, V> ShardedCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    pub fn try_add_arc2(&self, key: K, value: Arc<V>) -> bool
    {
        self.get_shard(&key).lock().unwrap().try_add_arc(key, value)
    }

    pub fn try_get2(&self, key: &K) -> Option<Arc<V>>
    {
        self.get_shard(&key).lock().unwrap().try_get(key)
    }

    pub fn new(shards: usize, max_size: usize, expiration: Duration, expiration_type: ExpirationType) -> Self
    {
        Self {
            shards: (0..shards)
                .map(|_| Arc::new(Mutex::new(ProbatoryCache::new(max_size, expiration, expiration_type))))
                .collect(),
        }
    }

    fn get_shard(&self, key: &K) -> &Arc<Mutex<ProbatoryCache<K, V>>>
    {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        let shard = hash % self.shards.len();
        &self.shards[shard]
    }

    pub async fn get_or_add_from_item2<I, Kfac, Vfac, Fut, E>(
        &self, item: I, key_factory: Kfac, value_factory: Vfac,
    ) -> Result<Arc<V>, E>
    where
        K: Clone,
        Kfac: Fn(&I) -> K,
        Vfac: FnOnce(I) -> Fut,
        Fut: Future<Output = Result<(V, bool), E>>,
    {
        let key = key_factory(&item);
        match self.try_get2(&key) {
            Some(value) => return Ok(value),
            None => {
                match value_factory(item).await {
                    Ok((value, cache)) => {
                        let a_value = Arc::new(value);
                        // There are cases where we want to return without caching the value, eg in 4xx errors.
                        if cache {
                            // This might fail if the key was added by another thread, but we don't care
                            // This is preferred over blocking the cache during the whole factory call duration
                            self.try_add_arc2(key.clone(), a_value.clone());
                        }
                        Ok(a_value)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn basic()
    {
        let mut lru = ShardedCache::new(1, 4, Duration::MAX, ExpirationType::Absolute);
        assert!(lru.try_get(&1).is_none());
        assert!(lru.try_add(1, "hello"));
        assert!(lru.try_get(&1).is_none(), "Key should only be in the probatory cache");
        assert!(lru.try_add(1, "hello"));
        assert!(lru.try_get(&1).is_some(), "Key should now be in the resident cache");
        assert!(!lru.try_add(1, "hello"));
    }

    #[test]
    fn trimming()
    {
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
