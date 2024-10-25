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
const SERVER_ADDR: &str = "172.18.0.1:8081"; // Update per server instance

// Counter for received images to create unique file names
static IMAGE_COUNTER: AtomicUsize = AtomicUsize::new(1);

// Function to create a new file for each image
async fn create_image_file() -> tokio::io::Result<File> {
    let image_num = IMAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let file_name = format!("received_image_{}.png", image_num);
    println!("Server: Creating file {}", file_name);
    File::create(file_name).await
}

async fn receive_chunk(socket: &UdpSocket, buffer: &mut [u8]) -> tokio::io::Result<(usize, std::net::SocketAddr)> {
    socket.recv_from(buffer).await
}

async fn send_ack(socket: &UdpSocket, src: std::net::SocketAddr, chunk_number: u32) -> tokio::io::Result<()> {
    let result = socket.send_to(ACK, src).await;
    if let Err(ref e) = result {
        println!("Server: Failed to send ACK for chunk {}: {:?}", chunk_number, e);
    } else {
        println!("Server: Sent ACK for chunk {}", chunk_number);
    }
    result.map(|_| ())  // Converts Result<usize, Error> to Result<(), Error>
}

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
        println!("Server: Unexpected chunk number {}, expected {}", chunk_number, *expected_chunk);
        Ok(false)
    }
}

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let socket = UdpSocket::bind(SERVER_ADDR).await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4];

    println!("Server: Ready to receive image chunks...");

    loop {
        // Wait for the first chunk (chunk 0) to start a new image transfer
        println!("Server: Waiting for the first chunk of a new image...");

        let mut expected_chunk = 0;
        let mut file = None;

        loop {
            let (bytes_received, src) = receive_chunk(&socket, &mut buffer).await?;
            let chunk_number = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
            let data = &buffer[4..bytes_received];

            if bytes_received < 4 {
                println!("Server: Packet too small to contain a chunk number, ignoring");
                continue;
            }

            // Check for chunk 0 to start a new image transfer
            if chunk_number == 0 {
                file = Some(Mutex::new(create_image_file().await?));
                expected_chunk = 0;
                println!("Server: New image transfer started. Reset expected_chunk to 0");
            }

            // Process chunks only if a file has been created for this transfer
            if let Some(ref f) = file {
                // Write chunk and send ACK if it's the expected chunk
                if write_chunk(f, &mut expected_chunk, chunk_number, data).await? {
                    send_ack(&socket, src, chunk_number).await?;
                } else {
                    println!("Server: Unexpected chunk number {}, expected {}", chunk_number, expected_chunk);
                }
            } else {
                println!("Server: No file created yet, ignoring chunk {}", chunk_number);
            }

            // Break if the transfer has ended (assumed on end of expected sequence)
            // Here you can add a condition to end the transfer based on application needs
        }
    }
}



