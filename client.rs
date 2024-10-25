use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::path::Path;
use steganography::encoder::*;
use steganography::util::{file_as_dynamic_image, save_image_buffer};
use tokio::time::{timeout, Duration,sleep};

const CHUNK_SIZE: usize = 1024; // Size of each chunk in bytes
const ACK: &[u8] = b"ACK";
const SERVER_ADDR: &str = "172.18.0.1:8008";
const CLIENT_ADDR: &str = "172.18.0.1:8081";

// Function to read chunks from the file
async fn read_chunk(file: &mut File, buffer: &mut [u8]) -> tokio::io::Result<usize> {
    file.read(buffer).await
}

// Sends a chunk to the server with a chunk number
async fn send_chunk(socket: &UdpSocket, chunk_number: u32, data: &[u8]) -> tokio::io::Result<()> {
    let packet = [&chunk_number.to_be_bytes()[..], data].concat();
    socket.send_to(&packet, SERVER_ADDR).await?;
    println!("Client: Sent chunk {}", chunk_number);
    Ok(())
}

// Receives an ACK from the server
async fn receive_ack(socket: &UdpSocket, chunk_number: u32) -> bool {
    let mut ack_buf = [0; 3];
    match timeout(Duration::from_secs(1), socket.recv_from(&mut ack_buf)).await {
        Ok(Ok((_, _))) if &ack_buf == ACK => {
            println!("Client: Received ACK for chunk {}", chunk_number);
            true
        }
        Ok(_) => {
            println!("Client: Received unexpected data, resending chunk {}", chunk_number);
            false
        }
        Err(_) => {
            println!("Client: No ACK received, resending chunk {}", chunk_number);
            false
        }
    }
}

// Main function for sending the encoded image in chunks
async fn send_image(file_path: &str) -> tokio::io::Result<()> {
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;
    let mut file = File::open(file_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    println!("Client: Sending image in chunks...");

    while let Ok(bytes_read) = read_chunk(&mut file, &mut buffer).await {
        if bytes_read == 0 {
            break; // End of file
        }

        loop {
            send_chunk(&socket, chunk_number, &buffer[..bytes_read]).await?;
            if receive_ack(&socket, chunk_number).await {
                chunk_number += 1;
                break;
            }
            sleep(Duration::from_millis(100)).await; // Wait before resending
        }
    }

    println!("Client: Image transfer complete!");
    Ok(())
}
#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let original_image_path = Path::new("original_image.jpg");
    let mask_image_path = Path::new("mask.jpeg");

    // Load and encode the original image
    let original_image = file_as_dynamic_image(original_image_path.to_str().unwrap().to_string());
    let mask_image = file_as_dynamic_image(mask_image_path.to_str().unwrap().to_string());
    let mut original_file = File::open(&original_image_path).await?;
    let mut original_bytes = Vec::new();
    original_file.read_to_end(&mut original_bytes).await?;
    
    // Steganographically encode the image
    let encoder = Encoder::new(&original_bytes, mask_image);
    let encoded_image = encoder.encode_alpha();  
    let encoded_image_path = "encoded_image.png";
    save_image_buffer(encoded_image, encoded_image_path.to_string());

    // Send the encoded image in chunks
    send_image(encoded_image_path).await
}
