use tokio::net::UdpSocket;
use std::error::Error;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration, sleep};
use steganography::decoder::*;
use steganography::util::file_as_dynamic_image;


const SERVER_ADDRS: [&str; 3] = ["172.18.0.1:8080", "172.18.0.1:8081", "172.18.0.1:8082"];
const CLIENT_ADDR: &str = "172.18.0.1:0";
const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const MAX_RETRIES: usize = 5;
const RETRY_DELAY: Duration = Duration::from_secs(2);


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // List of server addresses to query
    let servers = vec!["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];

    // Bind the client socket to an ephemeral port
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let request_message = "GET_CPU_USAGE";

    for server in &servers {
        // Send the CPU usage request to each server
        socket.send_to(request_message.as_bytes(), server).await?;
        println!("Sent CPU usage request to {}", server);
    }

    Ok(())
}
