use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::path::Path;
use steganography::encoder::*;
use steganography::util::{file_as_dynamic_image, save_image_buffer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?; // Bind to any available port for the client
    let server_addr = "127.0.0.1:8080";
    
    // Paths to original and mask images
    let original_image_path = Path::new("messi.jpg");
    let mask_image_path = Path::new("mask.png");

    // Load the original and mask images
    let original_image = file_as_dynamic_image(original_image_path.to_str().unwrap().to_string());
    let mask_image = file_as_dynamic_image(mask_image_path.to_str().unwrap().to_string());

    // Convert the original image into bytes
    let mut original_file = File::open(&original_image_path).await?;
    let mut original_bytes = Vec::new();
    original_file.read_to_end(&mut original_bytes).await?;
    
    // Encrypt the original image into the mask using steganography
    let encoder = Encoder::new(&original_bytes, mask_image);
    let encoded_image = encoder.encode_alpha();  // Embed using alpha channel

    // Save the encoded image to a file
    let encoded_image_path = "encoded_image.png";
    save_image_buffer(encoded_image, encoded_image_path.to_string());

    // Now send the encoded image to the server
    let mut encoded_file = File::open(&encoded_image_path).await?;
    println!("Opened encoded image: {:?}", encoded_image_path);

    // Send "START" signal before sending the image
    socket.send_to(b"START", server_addr).await?;
    println!("Sent start of transfer signal.");
    
    let mut buffer = [0u8; 1024];
    let mut total_bytes_sent = 0;

    // Read and send the encoded image file in chunks
    loop {
        let size = encoded_file.read(&mut buffer).await?;

        if size == 0 {
            println!("Finished reading the image.");
            break;
        }

        socket.send_to(&buffer[..size], server_addr).await?;
        total_bytes_sent += size;
        println!("Sent {} bytes", size);
    }

    // Send "END" signal to tell the server that the image transfer is complete
    socket.send_to(b"END", server_addr).await?;
    println!("Sent end of transfer signal. Total bytes sent: {}", total_bytes_sent);

    Ok(())
}
