use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::path::Path;
use steganography::decoder::*;
use steganography::util::file_as_dynamic_image;

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const SERVER_ADDR: &str = "172.18.0.1:8008";

// Counter for received images to create unique file names
static IMAGE_COUNTER: AtomicUsize = AtomicUsize::new(1);

// Creates a new file to save incoming image chunks
async fn create_image_file() -> tokio::io::Result<File> {
    let image_num = IMAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let file_name = format!("received_image_{}.png", image_num);
    println!("Server: Creating file {}", file_name);
    File::create(file_name).await
}

// Receives a chunk from the client
async fn receive_chunk(socket: &UdpSocket, buffer: &mut [u8]) -> tokio::io::Result<(usize, std::net::SocketAddr)> {
    socket.recv_from(buffer).await
}

// Sends an ACK back to the client
async fn send_ack(socket: &UdpSocket, src: std::net::SocketAddr, chunk_number: u32) -> tokio::io::Result<()> {
    socket.send_to(ACK, src).await?;
    println!("Server: Sent ACK for chunk {}", chunk_number);
    Ok(())
}

// Writes a chunk to the file if it’s the expected chunk
async fn write_chunk(
    file: &Mutex<File>,
    expected_chunk: &mut u32,
    chunk_number: u32,
    data: &[u8],
) -> tokio::io::Result<bool> {
    if chunk_number == *expected_chunk {
        println!("Server: Writing chunk {} to file", chunk_number);
        file.lock().unwrap().write_all(data).await?;
        *expected_chunk += 1;
        Ok(true)
    } else {
        println!("Server: Unexpected chunk number {}, expecting {}", chunk_number, *expected_chunk);
        Ok(false)
    }
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let socket = UdpSocket::bind(SERVER_ADDR).await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4];

    println!("Server: Ready to receive image chunks...");

    loop {
        let file = Mutex::new(create_image_file().await?);
        let mut expected_chunk = 0;

        loop {
            let (bytes_received, src) = receive_chunk(&socket, &mut buffer).await?;
            println!("Server: Received packet from {:?}", src);

            if bytes_received < 4 {
                println!("Server: Packet too small to contain a chunk number");
                continue;
            }

            let chunk_number = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
            let data = &buffer[4..bytes_received];

            if write_chunk(&file, &mut expected_chunk, chunk_number, data).await? {
                send_ack(&socket, src, chunk_number).await?;
            }
        }
    }
}