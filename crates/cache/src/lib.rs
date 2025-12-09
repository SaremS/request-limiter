use dashmap::DashSet;

mod storage;
use storage::{CacheStorage, InMemoryStorage};


#[derive(Debug)]
pub struct Cache<T: CacheStorage> {
    size: usize,
    ttl_seconds: usize,
    set: DashSet<String>,
    storage: T
}

impl Cache<InMemoryStorage> {
   pub fn new(size: &usize, ttl_seconds: &usize) -> Cache<InMemoryStorage> {
        Cache { 
            size: *size, 
            ttl_seconds: *ttl_seconds,
            set: DashSet::new(),
            storage: InMemoryStorage::new(),
        }
    } 
}

impl<T: CacheStorage> Cache<T> {
    pub fn get_size(&self) -> usize {
        self.size
    }

    pub fn get_ttl(&self) -> usize {
        self.ttl_seconds
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_size() {
        let cache: Cache<InMemoryStorage> = Cache::new(&10, &60);
        assert_eq!(cache.get_size(), 10);   
    }

    #[test]
    fn test_cache_size_zero() {
        let cache: Cache<InMemoryStorage> = Cache::new(&0, &60);
        assert_eq!(cache.get_size(), 0);
    }
}
