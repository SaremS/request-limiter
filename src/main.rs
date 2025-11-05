use std::error::Error;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;

use once_cell::sync::Lazy;
use url::Url; 
use dashmap::DashMap;
use clap::Parser;


static HOST_TIMESTAMPS: Lazy<DashMap<String, Instant>> =
    Lazy::new(DashMap::new);


//Forward proxy to throttle number of concurrent requests to the same host
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {

    //IP address to start the proxy at
    #[arg(short, long, default_value = "127.0.0.1")]
    ip: String,

    //Port for the proxy server
    #[arg(short, long, default_value_t = 8989)]
    port: u16,

    //Duration to wait between requests to the same host in ms
    #[arg(short, long, default_value_t = 500)]
    throttle_duration_ms: u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let server_address = format!("{}:{}", args.ip, args.port); 
    let throttling_throughput = 1000.0 / args.throttle_duration_ms as f64;

    let listener = TcpListener::bind(server_address.clone()).await?;
    println!("Proxy listening on {} (HTTP + HTTPS); Throttling to {:.2} requests/second", server_address, throttling_throughput);

    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        println!("Accepted connection from: {}", client_addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(client_stream, args.throttle_duration_ms).await {
                eprintln!("Failed to handle connection: {}", e);
            }
        });
    }
}

async fn throttle_host(host: &str, throttle_duration_ms: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
    let required_delay = Duration::from_millis(throttle_duration_ms);
    let mut wait_duration = Duration::from_secs(0);
    let now = Instant::now();

    let wait_duration = {
        //Fast path, no new String
        if let Some(mut entry) = HOST_TIMESTAMPS.get_mut(host) {
            let start_time = now.max(*entry);
            let new_start = start_time + required_delay;
            *entry = new_start; 
            start_time.duration_since(now) 
        } else {
            //Slow path, need to .to_string()
            let mut entry = HOST_TIMESTAMPS
                .entry(host.to_string()) // Allocate *only* on this miss
                .or_insert(now);

            let start_time = now.max(*entry);
            let new_start = start_time + required_delay;
            *entry = new_start;
            start_time.duration_since(now) 
        }
    }; 

    if !wait_duration.is_zero() {
        println!(
            "Throttling request to {}. Waiting for {:?}",
            host, wait_duration
        );
        tokio::time::sleep(wait_duration).await;
    }

    Ok(())
}

async fn handle_connection(
    client_stream: TcpStream,
    throttle_duration_ms: u64
) -> Result<(), Box<dyn Error + Send + Sync>> {
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

    match method {
        "CONNECT" => {
            let host = parts[1];
            println!("Handling CONNECT request to: {}", host);

            throttle_host(host, throttle_duration_ms).await?;

            loop {
                let mut line = String::new();
                if client_stream_reader.read_line(&mut line).await? == 0 { break; }
                if line.trim().is_empty() { break; }
            }

            println!("Connecting to target: {}", host);
            let mut target_stream = TcpStream::connect(host).await?;

            let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
            client_stream_reader
                .get_mut()
                .write_all(response.as_bytes())
                .await?;

            let mut client_stream = client_stream_reader.into_inner();
            println!("Tunneling data between client and {}", host);
            tokio::io::copy_bidirectional(&mut client_stream, &mut target_stream).await?;
            println!("Connection to {} closed.", host);
        }
        
        _ => {  
            let url_str = parts[1];
            println!("Handling HTTP request for: {}", url_str);

            let url = Url::parse(url_str)?;
            let target_host = url.host_str().ok_or("Invalid host in URL")?;
            let target_port = url.port_or_known_default().unwrap_or(80);
            let target_addr = format!("{}:{}", target_host, target_port);

            throttle_host(&target_addr, throttle_duration_ms).await?;

            println!("Connecting to target: {}", target_addr);
            let mut target_stream = TcpStream::connect(&target_addr).await?;

            let path = url.path(); 
            let path_and_query = match url.query() {
                Some(q) => format!("{}?{}", path, q),
                None => path.to_string(),
            };

            let new_request_line = format!("{} {} {}\r\n", method, path_and_query, parts[2]);
            target_stream.write_all(new_request_line.as_bytes()).await?;

            loop {
                let mut line = String::new();
                if client_stream_reader.read_line(&mut line).await? == 0 { break; }
                if line.trim().is_empty() {
                    target_stream.write_all(b"\r\n").await?;
                    break;
                }
                if !line.to_lowercase().starts_with("proxy-") {
                    target_stream.write_all(line.as_bytes()).await?;
                }
            }

            let mut client_stream = client_stream_reader.into_inner();
            println!("Forwarding body/response for {}", target_addr);
            tokio::io::copy_bidirectional(&mut client_stream, &mut target_stream).await?;
            println!("HTTP request to {} finished.", target_addr);
        }
    }

    Ok(())
}
