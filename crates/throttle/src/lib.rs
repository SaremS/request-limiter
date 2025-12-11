use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;

#[async_trait]
pub trait Throttle {
    fn get_throttle_duration(&self) -> u64;
    async fn set_throttle_duration(&mut self, duration_ms: u64);
    async fn throttle(&self, key: &str);
}

pub struct InMemoryThrottler {
    throttle_duration_ms: Duration,
    key_timestamps: DashMap<String, std::time::Instant>,
}

impl InMemoryThrottler {
    pub fn new(throttle_duration_ms: u64) -> Self {
        InMemoryThrottler {
            throttle_duration_ms: Duration::from_millis(throttle_duration_ms),
            key_timestamps: DashMap::new(),
        }
    }
}

#[async_trait]
impl Throttle for InMemoryThrottler {
    fn get_throttle_duration(&self) -> u64 {
        self.throttle_duration_ms.as_millis() as u64
    }

    async fn set_throttle_duration(&mut self, duration_ms: u64) {
        self.throttle_duration_ms = Duration::from_millis(duration_ms);
    }

    async fn throttle(&self, key: &str) {
        let now = std::time::Instant::now();
        let required_delay = self.throttle_duration_ms;

        let wait_duration = {
            if let Some(mut entry) = self.key_timestamps.get_mut(key) {
                let start_time = now.max(*entry);
                let new_start = start_time + required_delay;
                *entry = new_start;
                start_time.duration_since(now)
            } else {
                let new_start = now + required_delay;
                self.key_timestamps.insert(key.to_string(), new_start);
                Duration::from_secs(0)
            }
        };

        if wait_duration > Duration::from_secs(0) {
            tokio::time::sleep(wait_duration).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_throttler_creation() {
        let throttler = InMemoryThrottler::new(500);
        assert_eq!(throttler.get_throttle_duration(), 500);
    }

    #[tokio::test]
    async fn test_set_throttle_duration() {
        let mut throttler = InMemoryThrottler::new(500);
        throttler.set_throttle_duration(1000).await;
        assert_eq!(throttler.get_throttle_duration(), 1000);
    }

    #[tokio::test]
    async fn test_throttle() {
        let throttler = InMemoryThrottler::new(500);
        let start = std::time::Instant::now();
        throttler.throttle("test_key").await;
        let duration = start.elapsed();
        assert!(duration >= Duration::from_millis(0));
        assert!(duration < Duration::from_millis(500));

        let start = std::time::Instant::now();
        throttler.throttle("test_key").await;
        let duration = start.elapsed();
        assert!(duration >= Duration::from_millis(500));
    }
}
