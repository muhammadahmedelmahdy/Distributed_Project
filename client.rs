use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt}; // Importing necessary traits
use tokio::time::{timeout, Duration, sleep};
use std::error::Error;
use steganography::util::file_as_dynamic_image;
use steganography::decoder::Decoder;

const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];
const CLIENT_ADDR: &str = "127.0.0.1:0";
const CHUNK_SIZE: usize = 1024;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Bind the client socket to an ephemeral port
    let socket = UdpSocket::bind(CLIENT_ADDR).await?;
    let request_message = "REQUEST_LEADER";

    for server in &SERVER_ADDRS {
        // Send the CPU usage request to each server
        socket.send_to(request_message.as_bytes(), server).await?;
        println!("Sent CPU usage request to {}", server);
    }

    // Wait to receive the leader address
    let mut buffer = [0; 1024];
    match timeout(TIMEOUT_DURATION, socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, addr))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            if response.starts_with("LEADER,") {
                let leader_address = &response[7..]; // Extracting the leader address
                println!("Received leader address: {} from {}", leader_address, addr);

                // Start the image transfer process
                middleware_encrypt(&socket, leader_address).await?;
                
                // Receive the encrypted image back from the server
                // receive_image(&socket).await?;

                // //decode the received image
                // decode_image("encoded_image_received.png").await?;

                middleware_decrypt(&socket).await?;


            } else {
                println!("Unexpected response: {}", response);
            }
        },
        Ok(Err(e)) => {
            println!("Failed to receive response: {}", e);
        },
        Err(_) => {
            println!("Timeout waiting for leader response");
        },
    }

    Ok(())
}

async fn middleware_encrypt(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "original_image.jpg"; // Path to the image file
    let mut file = File::open(image_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    // Notify the server about the image transfer
    socket.send_to(b"IMAGE_TRANSFER", leader_addr).await?;
    println!("Client: Starting image transfer to {}", leader_addr);

    // Send the image in chunks
    while let Ok(bytes_read) = file.read(&mut buffer).await {
        if bytes_read == 0 { break; }

        loop {
            let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
            socket.send_to(&packet, leader_addr).await?;
            if receive_ack(socket).await { break; } // Wait for ACK before proceeding
            sleep(Duration::from_millis(100)).await; // Retry if no ACK
        }
        chunk_number += 1;
    }

    // Signal end of transfer
    socket.send_to(b"END", leader_addr).await?;
    println!("Client: Image transfer complete!");

    Ok(())
}

async fn receive_ack(socket: &UdpSocket) -> bool {
    let mut buffer = [0; 1024];

    match timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, _))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            return response.trim() == "ACK";
        },
        _ => false, // Timeout or error
    }
}

/// Receive image data from the server in chunks and save it as a PNG file.
async fn receive_image(socket: &UdpSocket) -> tokio::io::Result<()> {
    let mut file = File::create("encoded_image_received.png").await?;
    let mut buffer = [0u8; CHUNK_SIZE + 4]; // 4 extra bytes for chunk number
    let mut expected_chunk_number: u32 = 0;

    println!("Client: Waiting to receive encrypted image...");

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

/// Receive and decode an image from the server.
async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
    Ok(())
}