use async_trait::async_trait;

#[async_trait]
pub trait Logger {
    async fn log(&self, message: &str);
}

pub struct SimpleLogger;

impl SimpleLogger {
    pub fn new() -> Self {
        SimpleLogger {}
    }
}

impl Default for SimpleLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Logger for SimpleLogger {
    async fn log(&self, message: &str) {
        println!("[LOG]: {}", message);
    }
}
