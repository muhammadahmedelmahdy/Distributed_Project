use tokio::net::UdpSocket; //to provide UDP socket functionality
use tokio::fs::File; //to enable asynchronous file operations
use tokio::io::AsyncReadExt; //to enable asynchronous reading from a file
use std::path::Path; //for handling file paths in a platform-independent way
use steganography::encoder::*; //to encode an image
use steganography::util::{file_as_dynamic_image, save_image_buffer}; //to convert files to DynamicImage and save image buffers

#[tokio::main] //to enable the use of async/await in the main function
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = setup_socket("172.18.0.1:0").await?;
    let server_addr = "172.18.0.1:8080";

    let encoded_image_path = encode_image("original_image.jpg", "mask.jpeg").await?;

    send_image(&socket, server_addr, &encoded_image_path).await?;

    Ok(())
}

// setup_socket creates a UDP socket bound to a specified IP address. 
async fn setup_socket(bind_addr: &str) -> Result<UdpSocket, Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind(bind_addr).await?;
    Ok(socket)
}

// encode_image loads an image, applies steganographic encoding using another image as a mask, and saves the encoded image.
async fn encode_image( original_image_path: &str, mask_image_path: &str,) -> Result<String, Box<dyn std::error::Error>> {
    let original_image_path = Path::new(original_image_path);
    let mask_image_path = Path::new(mask_image_path);

    let original_image = file_as_dynamic_image(original_image_path.to_str().unwrap().to_string());
    let mask_image = file_as_dynamic_image(mask_image_path.to_str().unwrap().to_string());

    let mut original_file = File::open(&original_image_path).await?;
    let mut original_bytes = Vec::new();
    original_file.read_to_end(&mut original_bytes).await?;

    let encoder = Encoder::new(&original_bytes, mask_image);
    let encoded_image = encoder.encode_alpha();

    let encoded_image_path = "encoded_image.png".to_string();
    save_image_buffer(encoded_image, encoded_image_path.clone());

    Ok(encoded_image_path)
}

// send_image sends the encoded image over the UDP socket to server_addr
async fn send_image(socket: &UdpSocket, server_addr: &str, encoded_image_path: &str,) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoded_file = File::open(&encoded_image_path).await?;
    println!("Opened encoded image: {:?}", encoded_image_path);

    socket.send_to(b"START", server_addr).await?;
    println!("Sent start of transfer signal.");
    
    let mut buffer = [0u8; 1024];
    let mut sequence_number = 0u32;

    loop {
        let size = encoded_file.read(&mut buffer[4..]).await?;
        if size == 0 {
            println!("Finished reading the image.");
            break;
        }
        
        buffer[..4].copy_from_slice(&sequence_number.to_be_bytes());
        sequence_number += 1;

        socket.send_to(&buffer[..size + 4], server_addr).await?;
        println!("Sent packet with sequence number: {}", sequence_number);
    }

    socket.send_to(b"END", server_addr).await?;
    println!("Sent end of transfer signal.");

    Ok(())
}