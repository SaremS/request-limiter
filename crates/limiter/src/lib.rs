use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;
use tracing::info;
use url::Url;

use cache::Cache;
use cache::storage::{CacheStorage, InMemoryStorage, SimpleFileStorage};
use throttle::{InMemoryThrottler, Throttle};

#[async_trait]
pub trait Limiter {
    async fn run(self: Arc<Self>) {}
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
    ) -> Arc<Self> {
        Arc::new(Server {
            ip: ip.to_string(),
            port,
            cache: Cache::new(cache_size, cache_ttl_seconds),
            throttler: InMemoryThrottler::new(throttle_duration_ms),
        })
    }
}

impl<T: CacheStorage + Send + Sync, U: Throttle + Send + Sync> Server<T, U> {
    async fn handle_connection(
        &self,
        client_stream: TcpStream,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut client_stream_reader = BufReader::new(client_stream);

        let mut first_line = String::new();
        if client_stream_reader.read_line(&mut first_line).await? == 0 {
            return Err("Client closed connection prematurely".into());
        }

        let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
        if parts.len() < 3 {
            return Err("Invalid HTTP request line".into());
        }

        let method = parts[0].to_string();
        let host_or_url = parts[1].to_string();
        let version = parts[2].to_string();

        match method.as_str() {
            "CONNECT" => {
                self.handle_connect_method(&host_or_url, client_stream_reader, first_line)
                    .await
            }
            _ => {
                self.handle_else_methods(
                    &method,
                    &host_or_url,
                    &version,
                    client_stream_reader,
                    first_line,
                )
                .await
            }
        }
    }

    async fn handle_connect_method(
        &self,
        host: &str,
        mut client_stream_reader: BufReader<TcpStream>,
        first_line: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut request_buffer = first_line.clone();
        loop {
            let mut line = String::new();
            if client_stream_reader.read_line(&mut line).await? == 0 {
                break;
            }
            request_buffer.push_str(&line);
            if line.trim().is_empty() {
                break;
            }
        }

        let mut hasher = Sha256::new();
        let cache_key_str = format!("{}{}", host, request_buffer);
        hasher.update(cache_key_str.as_bytes());
        let cache_key = hex::encode(hasher.finalize());

        if let Some(cached_response) = self.cache.get(&cache_key).await {
            info!("Cache HIT for key: {}", cache_key);

            let stream = client_stream_reader.get_mut();
            stream.write_all(&cached_response).await?;
            stream.flush().await?;
            stream.shutdown().await?;

            return Ok(());
        }

        self.throttler.throttle(host).await;

        let target_stream = TcpStream::connect(host).await?;
        let (mut target_read, mut target_write) = tokio::io::split(target_stream);

        let connection_established = "HTTP/1.1 200 Connection Established\r\n\r\n";
        client_stream_reader
            .get_mut()
            .write_all(connection_established.as_bytes())
            .await?;

        let (mut client_read, mut client_write) = tokio::io::split(client_stream_reader);

        let upstream_task =
            tokio::spawn(async move { tokio::io::copy(&mut client_read, &mut target_write).await });

        let mut cache_buffer = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = target_read.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            client_write.write_all(&buffer[..n]).await?;
            cache_buffer.extend_from_slice(&buffer[..n]);
        }

        let _ = upstream_task.await;

        self.cache.put(&cache_key, &cache_buffer).await.ok();
        Ok(())
    }

    async fn handle_else_methods(
        &self,
        method: &str,
        url_str: &str,
        version: &str,
        mut client_stream_reader: BufReader<TcpStream>,
        first_line: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(url_str)?;
        let target_host = url.host_str().ok_or("Invalid host")?;
        let target_port = url.port_or_known_default().unwrap_or(80);
        let target_addr = format!("{}:{}", target_host, target_port);

        let mut headers_lines: Vec<String> = Vec::new();
        let mut full_request_str = first_line.clone();

        loop {
            let mut line = String::new();
            if client_stream_reader.read_line(&mut line).await? == 0 {
                break;
            }

            full_request_str.push_str(&line);
            headers_lines.push(line.clone());

            if line.trim().is_empty() {
                break;
            }
        }

        let mut hasher = Sha256::new();
        let cache_key_str = format!("{}{}", target_addr, full_request_str);
        hasher.update(cache_key_str.as_bytes());
        let cache_key = hex::encode(hasher.finalize());

        if let Some(cached_response) = self.cache.get(&cache_key).await {
            info!("Cache HIT for key: {}", cache_key);

            let stream = client_stream_reader.get_mut();
            stream.write_all(&cached_response).await?;
            stream.flush().await?;
            stream.shutdown().await?;

            return Ok(());
        }

        self.throttler.throttle(&target_addr).await;

        let mut target_stream = TcpStream::connect(&target_addr).await?;

        let path = url.path();
        let path_and_query = match url.query() {
            Some(q) => format!("{}?{}", path, q),
            None => path.to_string(),
        };

        let new_request_line = format!("{} {} {}\r\n", method, path_and_query, version);
        target_stream.write_all(new_request_line.as_bytes()).await?;

        for line in headers_lines {
            if !line.to_lowercase().starts_with("proxy-") {
                target_stream.write_all(line.as_bytes()).await?;
            }
        }

        let (mut target_read, mut target_write) = tokio::io::split(target_stream);
        let (mut client_read, mut client_write) = tokio::io::split(client_stream_reader);

        let upstream_task =
            tokio::spawn(async move { tokio::io::copy(&mut client_read, &mut target_write).await });

        let mut cache_buffer = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = target_read.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            client_write.write_all(&buffer[..n]).await?;
            cache_buffer.extend_from_slice(&buffer[..n]);
        }

        let _ = upstream_task.await;

        self.cache.put(&cache_key, &cache_buffer).await.ok();
        Ok(())
    }
}

#[async_trait]
impl<T: CacheStorage + Send + Sync + 'static, U: Throttle + Send + Sync + 'static> Limiter
    for Server<T, U>
{
    async fn run(self: Arc<Self>) {
        info!("Starting the server...");
        let listener = TcpListener::bind(format!("{}:{}", self.ip, self.port))
            .await
            .expect("Failed to bind to address");

        loop {
            let (client_stream, addr) = listener
                .accept()
                .await
                .expect("Failed to accept connection");

            let server_clone = self.clone();

            tokio::spawn(async move {
                if let Err(e) = server_clone.handle_connection(client_stream).await {
                    eprintln!("Error handling connection from {}: {}", addr, e);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_proxy_server_caches_requests() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();

        let hit_counter = Arc::new(AtomicUsize::new(0));
        let hit_counter_clone = hit_counter.clone();

        tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _)) = upstream_listener.accept().await {
                    let counter = hit_counter_clone.clone();
                    tokio::spawn(async move {
                        counter.fetch_add(1, Ordering::SeqCst);

                        let mut buf = [0u8; 1024];
                        let _ = socket.read(&mut buf).await;

                        let response = "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nHello World!";
                        socket.write_all(response.as_bytes()).await.unwrap();
                        socket.flush().await.unwrap();
                    });
                }
            }
        });

        let proxy_port = 9595;
        let server = Server::new_in_memory("127.0.0.1", proxy_port, &1024, &60, 10);

        let server_handle = tokio::spawn(async move {
            server.run().await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let proxy_url = format!("http://127.0.0.1:{}", proxy_port);
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(&proxy_url).unwrap())
            .build()
            .unwrap();

        let target_url = format!("http://{}/resource", upstream_addr);

        let res1 = client
            .get(&target_url)
            .send()
            .await
            .expect("Request 1 failed");
        let body1 = res1.text().await.expect("Failed to read body 1");

        assert_eq!(body1, "Hello World!");
        assert_eq!(
            hit_counter.load(Ordering::SeqCst),
            1,
            "Upstream should be hit once"
        );

        let res2 = client
            .get(&target_url)
            .send()
            .await
            .expect("Request 2 failed");
        let body2 = res2.text().await.expect("Failed to read body 2");

        assert_eq!(body2, "Hello World!");
        assert_eq!(
            hit_counter.load(Ordering::SeqCst),
            1,
            "Upstream should NOT be hit again (Cache Hit)"
        );

        server_handle.abort();
    }
}
