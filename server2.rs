use raft_lib::{RaftState, election_timeout, Role}; // Import necessary structs and functions
use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use steganography::decoder::*;
use steganography::util::file_as_dynamic_image;

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const SERVER_ADDR: &str = "172.18.0.1:8081"; // Update per server instance

static IMAGE_COUNTER: AtomicUsize = AtomicUsize::new(1);

async fn create_image_file() -> tokio::io::Result<(File, String)> {
    let image_num = IMAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let file_name = format!("received_image_{}.png", image_num);
    println!("Server: Creating file {}", file_name);
    let file = File::create(&file_name).await?;
    Ok((file, file_name))
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
    result.map(|_| ())
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

async fn decode_image(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Load the image as a DynamicImage and convert it to the required ImageBuffer format
    let encoded_image = file_as_dynamic_image(file_path.to_string()).to_rgba();
    let decoder = Decoder::new(encoded_image);
    
    // Decode the image data
    let decoded_data = decoder.decode_alpha();
    
    // Save the decoded data to an output file
    let output_path = "decoded_output.png";
    std::fs::write(output_path, &decoded_data)?;
    println!("Server: Image decoded and saved to {}", output_path);
    Ok(())
}




#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let socket = UdpSocket::bind(SERVER_ADDR).await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4];

    println!("Server: Ready to receive image chunks...");

    loop {
        println!("Server: Waiting for the first chunk of a new image...");

        let mut expected_chunk = 0;
        let mut total_chunks = None;
        let mut file = None;
        let mut file_path = String::new();

        loop {
            let (bytes_received, src) = receive_chunk(&socket, &mut buffer).await?;

            // Check for the end-of-transfer signal
            if bytes_received == 3 && &buffer[..3] == b"END" {
                println!("Server: End-of-transfer signal received. Starting decoding...");
                if expected_chunk == total_chunks.unwrap_or(0) {
                    if let Err(e) = decode_image(&file_path).await {
                        println!("Server: Error decoding image: {:?}", e);
                    }
                } else {
                    println!("Server: Incomplete file received, expected {} chunks but got {}", total_chunks.unwrap_or(0), expected_chunk);
                }
                break;
            }

            // First message with total chunk count
            if bytes_received == 4 && total_chunks.is_none() {
                total_chunks = Some(u32::from_be_bytes(buffer[0..4].try_into().unwrap()));
                println!("Server: Received total chunk count: {}", total_chunks.unwrap());
                continue;
            }

            if bytes_received < 4 {
                println!("Server: Packet too small to contain a chunk number, ignoring");
                continue;
            }

            let chunk_number = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
            let data = &buffer[4..bytes_received];

            // Create a new file if this is the first chunk
            if chunk_number == 0 && file.is_none() {
                let (f, path) = create_image_file().await?;
                file = Some(Mutex::new(f));
                file_path = path;
                expected_chunk = 0;
                println!("Server: New image transfer started. Reset expected_chunk to 0");
            }

            if let Some(ref f) = file {
                if write_chunk(f, &mut expected_chunk, chunk_number, data).await? {
                    send_ack(&socket, src, chunk_number).await?;
                }
            }
        }
    }
}