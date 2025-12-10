use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use dashmap::DashMap;

mod storage;
use storage::{CacheStorage, InMemoryStorage, SimpleFileStorage};

#[derive(Debug)]
pub struct Cache<T: CacheStorage> {
    size: AtomicUsize,
    ttl_seconds: AtomicU64,
    key_and_evict_map: DashMap<String, u64>,
    store: T,
}

impl Cache<InMemoryStorage> {
    pub fn new(size: &usize, ttl_seconds: &u64) -> Cache<InMemoryStorage> {
        Cache {
            size: (*size).into(),
            ttl_seconds: (*ttl_seconds).into(),
            key_and_evict_map: DashMap::new(),
            store: InMemoryStorage::new(),
        }
    }
}

impl Cache<SimpleFileStorage> {
    pub fn new_file_cache(size: &usize, ttl_seconds: &u64, path: &str) -> Cache<SimpleFileStorage> {
        Cache {
            size: (*size).into(),
            ttl_seconds: (*ttl_seconds).into(),
            key_and_evict_map: DashMap::new(),
            store: SimpleFileStorage::new(path),
        }
    }
}

impl<T: CacheStorage> Cache<T> {
    pub fn get_size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn get_ttl(&self) -> u64 {
        self.ttl_seconds.load(Ordering::Relaxed)
    }

    pub async fn set_size(&self, size: &usize) {
        self.size.store(*size, Ordering::Relaxed);
    }

    pub async fn set_ttl(&self, ttl_seconds: &u64) {
        self.ttl_seconds.store(*ttl_seconds, Ordering::Relaxed);
    }

    pub async fn put(&self, key: &str, value: &[u8]) -> Result<(), ()> {
        let now = Self::now_seconds();
        let evict_time = now + self.get_ttl();
        self.key_and_evict_map.insert(key.to_string(), evict_time);
        self.store.put(key, value).await
    }

    pub async fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let now = Self::now_seconds();
        let evict_time_opt = self.key_and_evict_map.get(key).map(|guard| *guard);
        if let Some(evict_time) = evict_time_opt {
            if evict_time > now {
                return self.store.get(key).await; //found and valid
            } else {
                self.store.delete(key).await.ok(); //expired
                self.key_and_evict_map.remove(key);
                return None; //found but expired
            }
        }
        None //Key not found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_size() {
        let cache: Cache<InMemoryStorage> = Cache::new(&10, &60);
        assert_eq!(cache.get_size(), 10);
    }

    #[tokio::test]
    async fn test_cache_size_zero() {
        let cache: Cache<InMemoryStorage> = Cache::new(&0, &60);
        assert_eq!(cache.get_size(), 0);
    }

    #[tokio::test]
    async fn test_put_get() {
        let cache: Cache<InMemoryStorage> = Cache::new(&10, &60);
        let key = "test_key";
        let value = b"test_value";

        cache.put(key, value).await.unwrap();
        let retrieved_value = cache.get(key).await;
        assert_eq!(retrieved_value, Some(Arc::new(value.to_vec())));
    }

    #[tokio::test]
    async fn test_get_expired() {
        let cache: Cache<InMemoryStorage> = Cache::new(&10, &1); // 1 second TTL
        let key = "test_key";
        let value = b"test_value";
        cache.put(key, value).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let retrieved_value = cache.get(key).await;
        assert_eq!(retrieved_value, None);
    }
}
