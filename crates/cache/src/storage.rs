use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;

#[async_trait]
pub trait CacheStorage {
    async fn put(&mut self, key: &str, value: &[u8]);
    async fn get(&self, key: &str) -> Option<Vec<u8>>;
    async fn delete(&mut self, key: &str);
}

#[derive(Debug)]
pub struct InMemoryStorage {
    storage: Arc<DashMap<String, Vec<u8>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage {
            storage: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CacheStorage for InMemoryStorage {
    async fn put(&mut self, key: &str, value: &[u8]) {
        self.storage.insert(key.to_string(), value.to_vec());        
    }
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.storage.get(key).map(|v| v.value().clone())
    }
    async fn delete(&mut self, key: &str) {
        self.storage.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_storage_put_get() {
        let mut storage = InMemoryStorage::new();
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value).await;
        let retrieved_value = storage.get(key).await;
        assert_eq!(retrieved_value, Some(value.to_vec()));
    }

    #[tokio::test]
    async fn test_in_memory_storage_delete() {
        let mut storage = InMemoryStorage::new();
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value).await;
        storage.delete(key).await;
        let retrieved_value = storage.get(key).await;
        assert_eq!(retrieved_value, None);
    }
}
