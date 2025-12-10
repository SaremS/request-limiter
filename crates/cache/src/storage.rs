use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait CacheStorage {
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), ()>;
    async fn get(&self, key: &str) -> Option<Arc<Vec<u8>>>;
    async fn delete(&self, key: &str) -> Result<(), ()>;
}

// In-memory implementation of CacheStorage using DashMap
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
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), ()> {
        self.storage.insert(key.to_string(), value.to_vec());
        return Ok(());
    }
    async fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.storage.get(key).map(|v| Arc::new(v.value().clone()))
    }
    async fn delete(&self, key: &str) -> Result<(), ()> {
        self.storage.remove(key);
        return Ok(());
    }
}

// File-based implementation of CacheStorage could be added here
pub struct SimpleFileStorage {
    path: String,
}

impl SimpleFileStorage {
    pub fn new(path: &str) -> Self {
        SimpleFileStorage {
            path: path.to_string(),
        }
    }
}

impl Default for SimpleFileStorage {
    fn default() -> Self {
        Self::new("cache_storage")
    }
}

#[async_trait]
impl CacheStorage for SimpleFileStorage {
    async fn put(&self, key: &str, value: &[u8]) -> Result<(), ()> {
        let file_path = format!("{}/{}", self.path, key);
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            fs::create_dir_all(parent).await.map_err(|_| ())?;
        }
        let mut file = fs::File::create(&file_path).await.map_err(|_| ())?;
        file.write_all(value).await.map_err(|_| ())?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let file_path = format!("{}/{}", self.path, key);
        match fs::read(&file_path).await {
            Ok(data) => Some(Arc::new(data)),
            Err(_) => None,
        }
    }

    async fn delete(&self, key: &str) -> Result<(), ()> {
        let file_path = format!("{}/{}", self.path, key);
        fs::remove_file(&file_path).await.map_err(|_| ())?;
        Ok(())
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
        assert_eq!(retrieved_value, Some(Arc::new(value.to_vec())));
    }

    #[tokio::test]
    async fn test_in_memory_storage_delete() {
        let storage = InMemoryStorage::new();
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value).await;
        storage.delete(key).await;
        let retrieved_value = storage.get(key).await;
        assert_eq!(retrieved_value, None);
    }

    #[tokio::test]
    async fn test_file_storage_put_get() {
        let storage = SimpleFileStorage::new("/tmp/test_cache_storage");
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value).await.unwrap();
        let retrieved_value = storage.get(key).await;
        assert_eq!(retrieved_value, Some(Arc::new(value.to_vec())));
    }

    #[tokio::test]
    async fn test_file_storage_delete() {
        let storage = SimpleFileStorage::new("/tmp/test_cache_storage");
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value).await.unwrap();
        storage.delete(key).await.unwrap();
        let retrieved_value = storage.get(key).await;
        assert_eq!(retrieved_value, None);
    }
}
