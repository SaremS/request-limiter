use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:4242").await?;
    println!("Minimal proxy listening on 127.0.0.1:4242");

    loop {
        let (client_stream, client_addr) = listener.accept().await?;
        println!("Accepted connection from: {}", client_addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(client_stream).await {
                eprintln!("Failed to handle connection: {}", e);
            }
        });
    }
}

async fn handle_connection(
    client_stream: TcpStream,
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
    let host = parts[1];

    if method != "CONNECT" {
        let response = "HTTP/1.1 501 Not Implemented\r\n\r\n";
        client_stream_reader
            .get_mut()
            .write_all(response.as_bytes())
            .await?;
        return Err("Only CONNECT method is supported".into());
    }

    loop {
        let mut line = String::new();
        if client_stream_reader.read_line(&mut line).await? == 0 {
            break; 
        }
        if line.trim().is_empty() {
            break; 
        }
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
    Ok(())
}
