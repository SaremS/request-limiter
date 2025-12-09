pub trait CacheStorage {
    fn put(&mut self, key: &str, value: &[u8]);
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn delete(&mut self, key: &str);
}

pub struct InMemoryStorage {
    storage: std::collections::HashMap<String, Vec<u8>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        InMemoryStorage {
            storage: std::collections::HashMap::new(),
        }
    }
}
impl CacheStorage for InMemoryStorage {
    fn put(&mut self, key: &str, value: &[u8]) {
        self.storage.insert(key.to_string(), value.to_vec());
    }
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.storage.get(key).cloned()
    }
    fn delete(&mut self, key: &str) {
        self.storage.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_storage_put_get() {
        let mut storage = InMemoryStorage::new();
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value);
        let retrieved_value = storage.get(key);
        assert_eq!(retrieved_value, Some(value.to_vec()));
    }

    #[test]
    fn test_in_memory_storage_delete() {
        let mut storage = InMemoryStorage::new();
        let key = "test_key";
        let value = b"test_value";
        storage.put(key, value);
        storage.delete(key);
        let retrieved_value = storage.get(key);
        assert_eq!(retrieved_value, None);
    }
}
