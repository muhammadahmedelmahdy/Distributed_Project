use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use steganography::decoder::*;
use steganography::util::file_as_dynamic_image;
use std::fs::write;  // Use this to save the decoded image data

// Custom function to save data to a file
fn save_data(data: Vec<u8>, file_path: &str) -> std::io::Result<()> {
    write(file_path, data)  // Save the data to the specified file
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080...");

    loop {
        let mut buffer = [0u8; 1024];
        let mut file = None; // File will be initialized when we receive the "START" signal

        loop {
            let (size, _addr) = socket.recv_from(&mut buffer).await?;

            // Check for "START" signal
            if &buffer[..size] == b"START" {
                // Create/truncate the file only when "START" signal is received
                file = Some(File::create("received_encoded_image.png").await?);
                println!("Received start of transfer signal. File created.");
                continue;
            }

            // Check for "END" signal
            if &buffer[..size] == b"END" {
                println!("End of image transfer signal received.");
                break;
            }

            // Write the received data to the file if it exists
            if let Some(f) = &mut file {
                f.write_all(&buffer[..size]).await?;
                println!("Received and wrote {} bytes to file", size);
            } else {
                println!("No file open. Waiting for START signal.");
            }
        }

        println!("Image transfer completed and saved as 'received_encoded_image.png'.");

        // Decode the hidden image after receiving the encoded image
        let encoded_image_path = "received_encoded_image.png";
        let encoded_image = file_as_dynamic_image(encoded_image_path.to_string());

        // Convert DynamicImage to ImageBuffer<Rgba<u8>, Vec<u8>> for the decoder
        let encoded_image_buffer = encoded_image.to_rgba();  // Use to_rgba instead of to_rgba8

        // Initialize the decoder and extract the hidden data
        let decoder = Decoder::new(encoded_image_buffer);
        let hidden_image_bytes = decoder.decode_alpha();  // Assuming it was encoded with the alpha channel

        // Save the extracted hidden image to a file
        let hidden_image_path = "extracted_hidden_image.jpeg"; // The original hidden image
        save_data(hidden_image_bytes, hidden_image_path)?;
        println!("Hidden image extracted and saved as '{}'", hidden_image_path);
    }
}
