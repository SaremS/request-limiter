use async_trait::async_trait;

use cache::Cache;
use cache::storage::{CacheStorage, InMemoryStorage, SimpleFileStorage};
use throttle::{InMemoryThrottler, Throttle};

pub mod logger;

use logger::{Logger, SimpleLogger};

#[async_trait]
pub trait Limiter<T: Logger + Send + Sync> {
    async fn run(&self) {}
}

pub struct Server<T: Logger + Send + Sync> {
    logger: T,
}

#[async_trait]
impl<T: Logger + Send + Sync> Limiter<T> for Server<T> {
    async fn run(&self) {
        self.logger.log("Server is running").await;
    }
}
