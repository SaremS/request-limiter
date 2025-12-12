use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;
use async_trait::async_trait;
use tracing::info;

use cache::Cache;
use cache::storage::{CacheStorage, InMemoryStorage, SimpleFileStorage};
use throttle::{InMemoryThrottler, Throttle};

#[async_trait]
pub trait Limiter {
    async fn run(&self) {}
}

pub struct Server<T, U>
where
    T: CacheStorage + Send + Sync,
    U: Throttle + Send + Sync,
{
    ip: String,
    port: u16,
    cache: Cache<T>,
    throttler: U,
}

impl Server<InMemoryStorage, InMemoryThrottler> {
    pub fn new_in_memory(
        ip: &str,
        port: u16,
        cache_size: &usize,
        cache_ttl_seconds: &u64,
        throttle_duration_ms: u64,
    ) -> Self {
        Server {
            ip: ip.to_string(),
            port,
            cache: Cache::new(cache_size, cache_ttl_seconds),
            throttler: InMemoryThrottler::new(throttle_duration_ms),
        }
    }
}

impl <T: CacheStorage + Send + Sync, U: Throttle + Send + Sync> Server<T, U> {
	async fn handle_connection(&self, client_stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut client_stream_reader = BufReader::new(client_stream);

        let mut first_line = String::new();

        if client_stream_reader.read_line(&mut first_line).await? == 0 {
            return Err("Client closed connection prematurely".into());
        }

        let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
        if parts.len() < 3 {
            return Err("Invalid HTTP request line".into());
        }

        let method = parts[0];
        let host = parts[1];

        match method {
            "CONNECT" => {
                info!("Handling CONNECT request to: {}", host);
                return self.handle_connect_method(host, client_stream_reader).await;
            }
            _ => {
                Err(format!("Unsupported HTTP method: {}", method).into())
            }
        }
    }

    async fn handle_connect_method(&self, host: &str, mut client_stream_reader: BufReader<TcpStream>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.throttler.throttle(host).await;

        loop {
            let mut line = String::new();
            if client_stream_reader.read_line(&mut line).await? == 0 { break; }
            if line.trim().is_empty() { break; }
        }

        let mut target_stream = TcpStream::connect(host).await?;

        let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
        client_stream_reader
            .get_mut()
            .write_all(response.as_bytes())
            .await?;

        let mut client_stream = client_stream_reader.into_inner();
        tokio::io::copy_bidirectional(&mut client_stream, &mut target_stream).await?;

        Ok(())
    }
}

#[async_trait]
impl<T: CacheStorage + Send + Sync, U: Throttle + Send + Sync> Limiter for Server<T, U> {
    async fn run(&self) {
        info!("Starting the server...");
    }
}
