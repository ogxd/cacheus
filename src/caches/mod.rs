pub mod lru;
pub use lru::LruCache;

pub mod probatory;
pub use probatory::ProbatoryCache;

pub mod sharded;
use std::{future::Future, sync::Arc};

pub use sharded::ShardedCache;

#[allow(async_fn_in_trait)]
pub trait Cache<K, V>
{
    fn len(&self) -> usize;

    fn try_add(&mut self, key: K, value: V) -> bool
    {
        self.try_add_arc(key, Arc::new(value))
    }

    fn try_add_arc(&mut self, key: K, value: Arc<V>) -> bool;

    fn try_get(&mut self, key: &K) -> Option<Arc<V>>;

    async fn get_or_add<Vfac, Fut>(&mut self, key: K, factory: Vfac) -> Result<Arc<V>, ()>
    where
        K: Clone,
        Vfac: Fn(&K) -> Fut,
        Fut: Future<Output = Result<V, ()>>,
    {
        self.get_or_add_from_item(key, |d: &K| d.clone(), factory).await
    }

    async fn get_or_add_from_item<I, Kfac, Vfac, Fut>(
        &mut self, item: I, key_factory: Kfac, value_factory: Vfac,
    ) -> Result<Arc<V>, ()>
    where
        K: Clone,
        Kfac: Fn(&I) -> K,
        Vfac: Fn(&I) -> Fut,
        Fut: Future<Output = Result<V, ()>>,
    {
        let key = key_factory(&item);
        match self.try_get(&key) {
            Some(value) => return Ok(value),
            None => {
                match value_factory(&item).await {
                    Ok(value) => {
                        let a_value = Arc::new(value);
                        // This might fail if the key was added by another thread, but we don't care
                        // This is preferred over blocking the cache during the whole factory call duration
                        self.try_add_arc(key.clone(), a_value.clone());
                        Ok(a_value)
                    }
                    Err(()) => Err(()),
                }
            }
        }
    }
}
