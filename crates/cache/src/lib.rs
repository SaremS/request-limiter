use std::time::SystemTime;

use dashmap::DashMap;

mod storage;
use storage::{CacheStorage, InMemoryStorage};

#[derive(Debug)]
pub struct Cache<T: CacheStorage> {
    size: usize,
    ttl_seconds: u64,
    key_and_evict_map: DashMap<String, u64>,
    store: T,
}

impl Cache<InMemoryStorage> {
    pub fn new(size: &usize, ttl_seconds: &u64) -> Cache<InMemoryStorage> {
        Cache {
            size: *size,
            ttl_seconds: *ttl_seconds,
            key_and_evict_map: DashMap::new(),
            store: InMemoryStorage::new(),
        }
    }
}

impl<T: CacheStorage> Cache<T> {
    pub async fn get_size(&self) -> usize {
        self.size
    }

    async fn now_seconds() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub async fn get_ttl(&self) -> u64 {
        self.ttl_seconds
    }

    pub async fn put(&mut self, key: &str, value: &[u8]) -> Result<(), ()> {
        let now = Self::now_seconds().await;
        let evict_time = now + self.ttl_seconds;
        self.key_and_evict_map.insert(key.to_string(), evict_time);
        self.store.put(key, value).await
    }

    pub async fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        let now = Self::now_seconds().await;
        let evict_time_opt = self.key_and_evict_map.get(key);
        if let Some(evict_time) = evict_time_opt {
            if *evict_time > now {
                return self.store.get(key).await; //found and not expired
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
        assert_eq!(cache.get_size().await, 10);
    }

    #[tokio::test]
    async fn test_cache_size_zero() {
        let cache: Cache<InMemoryStorage> = Cache::new(&0, &60);
        assert_eq!(cache.get_size().await, 0);
    }
}
