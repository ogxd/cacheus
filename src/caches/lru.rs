use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::hash::Hash;

use serde::ser::SerializeStruct;
use serde::{Serialize, Deserialize};

use crate::caches::Cache;
use crate::ArenaLinkedList;

#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
pub struct LruCache<K, V>
where
    K: Eq + Hash + Serialize,
    V: Serialize,
{
    lru_list: ArenaLinkedList<K>,
    map: HashMap<K, LruCacheEntry<V>>,
    expiration: Duration,
    expiration_type: ExpirationType,
    max_size: usize,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum ExpirationType
{
    Absolute,
    Sliding,
}

//#[derive(Serialize, Deserialize)]
struct LruCacheEntry<V>
{
    node_index: usize,
    insertion: Instant,
    value: Arc<V>,
}

impl<V: Serialize> Serialize for LruCacheEntry<V> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("LruCacheEntry", 3)?;
        s.serialize_field("node_index", &self.node_index)?;

        // Instant isn't serializable — store a relative timestamp instead
        let since_creation = self.insertion.elapsed().as_secs_f64();
        s.serialize_field("insertion_elapsed_secs", &since_creation)?;

        // Arc<V> isn't serializable directly — serialize inner V
        s.serialize_field("value", &*self.value)?;
        s.end()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for LruCacheEntry<V> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper<V> {
            node_index: usize,
            insertion_elapsed_secs: f64,
            value: V,
        }

        let h = Helper::deserialize(deserializer)?;
        Ok(Self {
            node_index: h.node_index,
            insertion: Instant::now() - Duration::from_secs_f64(h.insertion_elapsed_secs),
            value: Arc::new(h.value),
        })
    }
}

impl<K: Eq + Hash + Serialize + Clone, V: Serialize> Cache<K, V> for LruCache<K, V>
{
    fn len(&self) -> usize
    {
        self.map.len()
    }

    fn try_add(&mut self, key: K, value: V) -> bool
    {
        let mut added = false;

        self.map.entry(key).or_insert_with_key(|k| {
            added = true;
            LruCacheEntry {
                node_index: self.lru_list.add_last(k.clone()).expect("Failed to add node to list"),
                insertion: Instant::now(),
                value: Arc::new(value),
            }
        });

        if added {
            self.trim();
        }

        return added;
    }

    fn try_get(&mut self, key: &K) -> Option<Arc<V>>
    {
        // If found in the map, remove from the lru list and reinsert at the end
        let lru_list = &mut self.lru_list;

        match self.map.entry(key.clone()) {
            Entry::Occupied(mut entry) => {
                if Instant::now() - entry.get().insertion > self.expiration {
                    // Entry has expired, we remove it and pretend it's not in the cache
                    lru_list
                        .remove(entry.get().node_index)
                        .expect("Failed to remove node, cache is likely corrupted");
                    entry.remove_entry();
                    None
                } else {
                    if self.expiration_type == ExpirationType::Sliding {
                        // Refresh duration
                        entry.get_mut().insertion = Instant::now();
                    }

                    // Move to the end of the list (the "LRU" part)
                    lru_list
                        .remove(entry.get().node_index)
                        .expect("Failed to remove node, cache is likely corrupted");
                    entry.get_mut().node_index = lru_list
                        .add_last(key.clone())
                        .expect("Failed to add node to list, cache is likely corrupted");

                    Some(entry.into_mut().value.clone())
                }
            }
            Entry::Vacant(_) => None,
        }
    }
}

#[allow(dead_code)]
impl<K: Eq + Hash + Serialize, V: Serialize> LruCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
{
    pub fn new(max_size: usize, expiration: Duration, expiration_type: ExpirationType) -> Self
    {
        Self {
            lru_list: ArenaLinkedList::new_with_capacity(max_size),
            map: HashMap::new(),
            expiration: expiration,
            expiration_type: expiration_type,
            max_size: max_size,
        }
    }

    fn trim(&mut self)
    {
        let mut index = self.lru_list.get_first_index().unwrap_or(usize::MAX);
        while index != usize::MAX {
            let node = self
                .lru_list
                .get(index)
                .expect("Failed to get node, cache is likely corrupted");
            let key = node
                .get_value()
                .as_ref()
                .expect("Node has no value, cache is likely corrupted")
                .clone();
            let entry = self
                .map
                .get(&key)
                .expect("Node not found in map, cache is likely corrupted");
            let next_index = node.get_after_index();
            if self.lru_list.count() > self.max_size || Instant::now() - entry.insertion > self.expiration {
                self.map.remove(&key);
                self.lru_list
                    .remove(index)
                    .expect("Failed to remove node, cache is likely corrupted");
            } else {
                break;
            }
            index = next_index;
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::CacheEnum;

    use super::*;

    #[test]
    fn basic()
    {
        let mut lru: CacheEnum<u32, &str> = CacheEnum::Lru(LruCache::new(4, Duration::MAX, ExpirationType::Absolute));
        assert!(lru.try_get(&1).is_none());
        assert!(lru.try_add(1, "hello"));
        assert!(!lru.try_add(1, "hello"));
        assert!(lru.try_get(&1).is_some());
    }

    #[test]
    fn trimming()
    {
        let mut lru = CacheEnum::Lru(LruCache::new(4, Duration::MAX, ExpirationType::Absolute));
        assert!(lru.try_add(1, "h"));
        assert!(lru.try_add(2, "e"));
        assert!(lru.try_add(3, "l"));
        assert!(lru.try_add(4, "l"));
        // Max size is reached, next insertion should evict oldest entry, which is 1
        assert!(lru.try_add(5, "o"));
        assert!(lru.try_get(&1).is_none());
        assert!(lru.try_get(&2).is_some());
        assert!(lru.try_get(&3).is_some());
        assert!(lru.try_get(&4).is_some());
        assert!(lru.try_get(&5).is_some());
    }

    #[test]
    fn reordering()
    {
        let mut lru = CacheEnum::Lru(LruCache::new(4, Duration::MAX, ExpirationType::Absolute));
        assert!(lru.try_add(1, "h"));
        assert!(lru.try_add(2, "e"));
        assert!(lru.try_add(3, "l"));
        assert!(lru.try_add(4, "l"));
        assert!(lru.try_get(&1).is_some());
        // Max size is reached, next insertion should evict oldest entry, which is 2, because 1 was just accessed
        assert!(lru.try_add(5, "o"));
        assert!(lru.try_get(&1).is_some());
        assert!(lru.try_get(&2).is_none());
        assert!(lru.try_get(&3).is_some());
        assert!(lru.try_get(&4).is_some());
        assert!(lru.try_get(&5).is_some());
    }
}