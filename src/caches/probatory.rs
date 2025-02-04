use std::{sync::Arc, time::Duration};

use super::lru::ExpirationType;
use crate::{Cache, LruCache};

#[allow(dead_code)]
pub struct ProbatoryCache<K, V>
{
    probatory: Option<LruCache<K, ()>>,
    resident: LruCache<K, V>,
}

impl<K, V> Cache<K, V> for ProbatoryCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn len(&self) -> usize
    {
        self.resident.len()
    }

    fn try_add_arc(&mut self, key: K, value: Arc<V>) -> bool {
        // Try to add to probatory cache first (if it exists)
        let is_new_to_probatory = self.probatory
            .as_mut()
            .map_or(false, |probatory| probatory.try_add(key.clone(), ()));

        if is_new_to_probatory {
            return true;
        }

        // If not new to probatory or probatory doesn't exist, try resident cache
        self.resident.try_add_arc(key, value)
    }

    fn try_get(&mut self, key: &K) -> Option<Arc<V>>
    {
        self.resident.try_get(key)
    }
}

#[allow(dead_code)]
impl<K, V> ProbatoryCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    pub fn new(probatory_size: usize, resident_size: usize, expiration: Duration, expiration_type: ExpirationType) -> Self
    {
        Self {
            probatory: if probatory_size > 0 { Some(LruCache::new(probatory_size, expiration, ExpirationType::Sliding)) } else { None },
            resident: LruCache::new(resident_size, expiration, expiration_type),
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
        let mut lru = ProbatoryCache::new(40, 4, Duration::MAX, ExpirationType::Absolute);
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
        let mut lru = ProbatoryCache::new(40, 4, Duration::MAX, ExpirationType::Absolute);
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
