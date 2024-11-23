use tokio::net::UdpSocket;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration, sleep};
use std::error::Error;
use steganography::util::file_as_dynamic_image;
use steganography::decoder::Decoder;
use image::open;
use reqwest::Client;
use tokio_tungstenite::connect_async;
use futures_util::stream::StreamExt;
use serde_json::json;
use image::{io::Reader as ImageReader, DynamicImage, ImageOutputFormat};
use base64::encode;
use std::io::Cursor;


const SERVER_ADDRS: [&str; 3] = ["127.0.0.1:8080", "127.0.0.1:8081", "127.0.0.1:8082"];
const CLIENT_ADDR: &str = "127.0.0.1:0";
const CHUNK_SIZE: usize = 1024;
const TIMEOUT_DURATION: Duration = Duration::from_secs(10);
const ACK: &[u8] = b"ACK";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let socket = UdpSocket::bind(CLIENT_ADDR).await?;

    // // Step 1: Send "IMAGE_TRANSFER" request to initiate election
    // for server in &SERVER_ADDRS {
    //     socket.send_to(b"IMAGE_TRANSFER", server).await?;
    //     println!("Sent image transfer request to {}", server);
    // }

    // // Step 2: Wait for the elected leader's response
    // let elected_leader = wait_for_leader(&socket).await?;
    // println!("Elected leader is {}", elected_leader);

    // // Step 3: Send image data only to the elected leader
    // send_image_data(&socket, &elected_leader).await?;

    // // Step 4: Receive the encrypted image from the elected leader
    // middleware_decrypt(&socket).await?;

    // let dos_address = "http://localhost:3030";
    // let client_id = "client1";
    // let image_path = "image3.jpg";

    // println!("Uploading image '{}' for client '{}' to the DoS at {}", image_path, client_id, dos_address);

    // match add_image_to_dos(dos_address, client_id, image_path).await {
    //     Ok(response) => println!("Success: {}", response),
    //     Err(e) => eprintln!("Failed to upload image: {}", e),
    // }
    // let dos_address = "http://localhost:3030";

    // // Example client ID and image name to delete
    // let client_id = "client1";
    // let image_name = "messi.jpg";

    // // Inform the user about the operation
    // println!(
    //     "Attempting to delete image '{}' for client '{}' from the DoS at {}",
    //     image_name, client_id, dos_address
    // );

    // // Call the delete_image_from_dos function
    // match delete_image_from_dos(dos_address, client_id, image_name).await {
    //     Ok(response) => println!("Image deleted successfully: {}", response),
    //     Err(e) => eprintln!("Failed to delete image: {}", e),
    // }

    // DoS base URL
    let dos_address = "http://localhost:3030";

    // Client ID for which the composite image is being fetched
    let client_id = "client1";

    // Output path for the composite image
    let output_path = "composite_image.png";

    // Fetch and save the composite image
    if let Err(e) = fetch_composite_image(dos_address, client_id, output_path).await {
        eprintln!("Error fetching composite image: {}", e);
    }

    

    Ok(())
}

async fn wait_for_leader(socket: &UdpSocket) -> Result<String, Box<dyn Error>> {
    let mut buffer = [0; 1024];
    loop {
        if let Ok((len, addr)) = timeout(TIMEOUT_DURATION, socket.recv_from(&mut buffer)).await? {
            let response = String::from_utf8_lossy(&buffer[..len]);
            if response.starts_with("LEADER,") {
                let leader_address = response[7..].to_string();
                println!("Received leader confirmation from {}", leader_address);
                return Ok(leader_address);
            }
        }
    }
}

async fn send_image_data(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "original_image.jpg";
    let mut file = File::open(image_path).await?;
    let mut buffer = [0u8; CHUNK_SIZE];
    let mut chunk_number: u32 = 0;

    println!("Client: Sending image data to leader at {}", leader_addr);

    // Send the image in chunks to the elected leader
    while let Ok(bytes_read) = file.read(&mut buffer).await {
        if bytes_read == 0 { break; }

        let packet = [&chunk_number.to_be_bytes(), &buffer[..bytes_read]].concat();
        loop {
            socket.send_to(&packet, leader_addr).await?;
            if receive_ack(socket).await { break; }
            sleep(Duration::from_millis(100)).await;
        }
        chunk_number += 1;
    }

    socket.send_to(b"END", leader_addr).await?;
    println!("Client: Image transfer complete!");

    Ok(())
}

async fn receive_ack(socket: &UdpSocket) -> bool {
    let mut buffer = [0; 1024];
    match timeout(Duration::from_secs(2), socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, _))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            response.trim() == "ACK"
        },
        _ => false,
    }
}

async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
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






/// Adds an image to the Directory of Service (DoS) after resizing it and encoding it as Base64.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client uploading the image.
/// * `image_path` - The path to the image file.
///
/// # Returns
/// A `Result` containing the response from the DoS or an error if the request fails.
pub async fn add_image_to_dos(
    dos_address: &str,
    client_id: &str,
    image_path: &str,
) -> Result<String, Box<dyn Error>> {
    // Create a new HTTP client
    let client = Client::new();

    // Load and process the image (resize and encode as Base64)
    let resized_image = resize_image(image_path)?;
    let base64_image = encode_image_to_base64(&resized_image)?;

    // Construct the request payload
    let payload = json!({
        "client_id": client_id,
        "image_name": image_path, // Keep the original image name
        "image_data": base64_image // Include the Base64-encoded image
    });

    // Send the POST request to the DoS
    let response = client
        .post(format!("{}/add_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    // Check if the response status is success
    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to add image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

/// Resizes the image to a lower resolution.
///
/// # Arguments
/// * `image_path` - The path to the image file.
///
/// # Returns
/// A `DynamicImage` containing the resized image.
fn resize_image(image_path: &str) -> Result<DynamicImage, Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?;
    let resized_image = image.resize_exact(200, 200, image::imageops::FilterType::Triangle);
    Ok(resized_image)
}

/// Encodes a `DynamicImage` as a Base64 string in PNG format.
///
/// # Arguments
/// * `image` - The `DynamicImage` to encode.
///
/// # Returns
/// A Base64 string representation of the image.
fn encode_image_to_base64(image: &DynamicImage) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer); // Wrap Vec<u8> in Cursor
    image.write_to(&mut cursor, ImageOutputFormat::Png)?;
    Ok(base64::encode(&buffer))
}


/// Deletes an image from the Directory of Service (DoS) by its name.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client deleting the image.
/// * `image_name` - The name of the image to delete.
///
/// # Returns
/// A `Result` containing the server's response message or an error if the request fails.
pub async fn delete_image_from_dos(
    dos_address: &str,
    client_id: &str,
    image_name: &str,
) -> Result<String, Box<dyn Error>> {
    // Create a new HTTP client
    let client = Client::new();

    // Construct the request payload
    let payload = json!({
        "client_id": client_id,
        "image_name": image_name,
    });

    // Send the POST request to the DoS
    let response = client
        .post(format!("{}/delete_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    // Handle response based on HTTP status
    if response.status().is_success() {
        // Return the server's response message
        let response_text = response.text().await?;
        println!("Server response: {}", response_text);
        Ok(response_text)
    } else {
        // Handle errors with detailed message
        let status = response.status();
        let error_body = response.text().await.unwrap_or_else(|_| "No response body".to_string());
        Err(format!(
            "Failed to delete image (status: {}): {}",
            status, error_body
        )
        .into())
    }
}



/// Fetches the composite image for a specific client from the DoS and saves it locally.
///
/// # Arguments
/// * `dos_address` - The base URL of the DoS server (e.g., "http://localhost:3030").
/// * `client_id` - The ID of the client whose images should be listed.
/// * `output_path` - The path where the composite image should be saved.
///
/// # Returns
/// A `Result` indicating success or failure.
pub async fn fetch_composite_image(
    dos_address: &str,
    client_id: &str,
    output_path: &str,
) -> Result<(), Box<dyn Error>> {
    // Create an HTTP client
    let client = Client::new();

    // Construct the URL with query parameters
    let url = format!("{}/list_by_client?client_id={}", dos_address, client_id);

    // Send the GET request
    let response = client.get(&url).send().await?;

    // Check if the response status is success
    if response.status().is_success() {
        let image_bytes = response.bytes().await?;
        // Save the image to the specified output path
        let mut file = File::create(output_path).await?;
        file.write_all(&image_bytes).await?;
        println!("Composite image saved to {}", output_path);
        Ok(())
    } else {
        Err(format!(
            "Failed to fetch composite image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
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





