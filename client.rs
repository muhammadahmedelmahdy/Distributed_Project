use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration, sleep};
use steganography::decoder::*;
use steganography::util::file_as_dynamic_image;
use std::error::Error;

const SERVER_ADDRS: [&str; 3] = ["172.18.0.1:8080", "172.18.0.1:8081", "172.18.0.1:8082"];
const CLIENT_ADDR: &str = "172.18.0.1:0";
const CHUNK_SIZE: usize = 1024;
const ACK: &[u8] = b"ACK";
const MAX_RETRIES: usize = 5;
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// Verify and return the address of the leader server, if available.
async fn verify_leader(socket: &UdpSocket) -> Option<String> {
    for attempt in 0..MAX_RETRIES {
        for &server_addr in &SERVER_ADDRS {
            socket.send_to(b"REQUEST_LEADER", server_addr).await.ok()?;
            let mut buffer = [0; 15];
            if let Ok(Ok((bytes, addr))) = timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
                if &buffer[..bytes] == b"LEADER_CONFIRM" {
                    println!("Client: Leader confirmed at {}", addr);
                    return Some(addr.to_string());
                }
            }
        }
        println!("Client: Attempt {} - No leader found, retrying...", attempt + 1);
        sleep(RETRY_DELAY).await;
    }
    println!("Client: Leader could not be verified after {} attempts.", MAX_RETRIES);
    None
}

/// Wait for acknowledgment (ACK) for the specified chunk.
async fn receive_ack(socket: &UdpSocket, chunk_number: u32) -> bool {
    let mut ack_buf = [0; 3];
    match timeout(Duration::from_secs(5), socket.recv_from(&mut ack_buf)).await {
        Ok(Ok((_, _))) if &ack_buf == ACK => {
            println!("Client: Received ACK for chunk {}", chunk_number);
            true
        }
        _ => {
            println!("Client: No ACK for chunk {}", chunk_number);
            false
        }
    }
}

/// Encrypt and transfer an image to the leader server in chunks.
async fn middleware_encrypt(socket: &UdpSocket) -> tokio::io::Result<()> {
    if let Some(leader_addr) = verify_leader(socket).await {
        let image_path = "original_image.jpg";
        let mut file = File::open(image_path).await?;
        let mut buffer = [0u8; CHUNK_SIZE];
        let mut chunk_number: u32 = 0;

        // Notify the server about the image transfer
        socket.send_to(b"IMAGE_TRANSFER", &leader_addr).await?;
        println!("Client: Starting image transfer to {}", leader_addr);

        // Send the image in chunks
        while let Ok(bytes_read) = file.read(&mut buffer).await {
            if bytes_read == 0 { break; }

            loop {
                let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
                socket.send_to(&packet, &leader_addr).await?;
                if receive_ack(socket, chunk_number).await { break; }
                sleep(Duration::from_millis(100)).await;
            }
            chunk_number += 1;
        }

        socket.send_to(b"END", &leader_addr).await?;
        println!("Client: Image transfer complete!");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;
    middleware_encrypt(&socket).await?;
    middleware_decrypt(&socket).await?;
    Ok(())
}

/// Receive and decode an image from the server.
async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
    Ok(())
}

/// Receive image data from the server in chunks and save it as a PNG file.
async fn receive_image(socket: &UdpSocket) -> tokio::io::Result<()> {
    let mut file = File::create("encoded_image_received.png").await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4]; // 4 extra bytes for chunk number
    let mut expected_chunk_number: u32 = 0;

    loop {
        let (bytes_received, addr) = socket.recv_from(&mut buffer).await?;

        if &buffer[..bytes_received] == b"END" {
            println!("Client: Received end of transmission signal.");
            break;
        }

        let chunk_number = u32::from_be_bytes(buffer[..4].try_into().unwrap());

        if chunk_number == expected_chunk_number {
            file.write_all(&buffer[4..bytes_received]).await?;
            expected_chunk_number += 1;

            socket.send_to(ACK, addr).await?;
            println!("Client: Acknowledged chunk {}", chunk_number);
        } else {
            println!("Client: Unexpected chunk number. Expected {}, but received {}.", expected_chunk_number, chunk_number);
        }
    }

    println!("Client: Image successfully received and saved as PNG.");
    Ok(())
}

/// Decode the received image and save the decoded output.
async fn decode_image(file_path: &str) -> Result<(), Box<dyn Error>> {
    let encoded_image = file_as_dynamic_image(file_path.to_string()).to_rgba();
    let decoder = Decoder::new(encoded_image);

    let decoded_data = decoder.decode_alpha();
    let output_path = "decoded_output.png";
    std::fs::write(output_path, &decoded_data)?;
    println!("Image decoded and saved to {}", output_path);
    Ok(())
}
