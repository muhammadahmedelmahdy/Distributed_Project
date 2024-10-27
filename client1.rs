use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::path::Path;
use steganography::encoder::*;
use steganography::util::{file_as_dynamic_image, save_image_buffer};
use tokio::time::{timeout, Duration, sleep};
use rand::prelude::SliceRandom;

const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const CLIENT_ADDR: &str = "172.18.0.1:0";
const SERVER_ADDRS: [&str; 3] = ["172.18.0.1:8080", "172.18.0.1:8081", "172.18.0.1:8082"];

// Function to read chunks from the file
async fn read_chunk(file: &mut File, buffer: &mut [u8]) -> tokio::io::Result<usize> {
    file.read(buffer).await
}

// Helper function to send a chunk to the chosen server
async fn send_chunk_to_server(socket: &UdpSocket, chunk_number: u32, data: &[u8], server_addr: &str) -> tokio::io::Result<()> {
    let packet = [&chunk_number.to_be_bytes()[..], data].concat();
    socket.send_to(&packet, server_addr).await?;
    println!("Client: Sent chunk {} to {}", chunk_number, server_addr);
    Ok(())
}

// Receives an ACK from the server
async fn receive_ack(socket: &UdpSocket, chunk_number: u32) -> bool {
    let mut ack_buf = [0; 3];
    // Increase the timeout duration to 5 seconds temporarily
    match timeout(Duration::from_secs(5), socket.recv_from(&mut ack_buf)).await {
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


async fn send_image(file_path: &str) -> tokio::io::Result<()> {
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;
    let mut file = File::open(file_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    // Calculate the total number of chunks
    let metadata = tokio::fs::metadata(file_path).await?;
    let total_chunks = (metadata.len() as f32 / CHUNK_SIZE as f32).ceil() as u32;

    // Choose one server for the entire image transfer
    let server_addr = SERVER_ADDRS.choose(&mut rand::thread_rng()).unwrap();
    println!("Client: Sending image to server {}", server_addr);

    // Send the total chunk count as the first message
    socket.send_to(&total_chunks.to_be_bytes(), server_addr).await?;
    println!("Client: Sent total chunk count: {}", total_chunks);

    println!("Client: Sending image in chunks...");

    while let Ok(bytes_read) = read_chunk(&mut file, &mut buffer).await {
        if bytes_read == 0 {
            break; // End of file
        }

        loop {
            send_chunk_to_server(&socket, chunk_number, &buffer[..bytes_read], server_addr).await?;
            if receive_ack(&socket, chunk_number).await {
                chunk_number += 1;
                break;
            }
            sleep(Duration::from_millis(100)).await; // Wait before resending
        }
    }

    // Send the end-of-transfer signal after all chunks are sent
    socket.send_to(b"END", server_addr).await?;
    println!("Client: Sent end-of-transfer signal.");

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

