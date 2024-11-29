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
use std::net::{IpAddr, Ipv4Addr,  UdpSocket as StdUdpSocket};


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
    let mut dos_port: Option<u16> = None;
    match timeout(TIMEOUT_DURATION, socket.recv_from(&mut buffer)).await {
        Ok(Ok((len, addr))) => {
            let response = String::from_utf8_lossy(&buffer[..len]);
            if response.starts_with("LEADER,") {
                let leader_address = &response[7..]; // Extracting the leader address
                println!("Received leader address: {} from {}", leader_address, addr);
                // Determine the DOS port based on the leader's port
            let leader_port: u16 = leader_address.split(':')
            .nth(1) // Get the port part of the address
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        dos_port = match leader_port {
            8080 => Some(8083),
            8081 => Some(8084),
            8082 => Some(8085),
            _ => {
                eprintln!("Unknown leader port: {}", leader_port);
                None
            }
        };

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
    if let Some(port) = dos_port {
        let dos_address = format!("http://localhost:{}", port);
        println!("DOS Address determined: {}", dos_address);
    
        // // Use the dos_address for further actions
        let client_id = "sisi";
        let password = "sisira2esy";
        let image_path = "image2.jpg";
    
        match add_image_to_dos(&dos_address, client_id, password, image_path).await {
            Ok(response) => println!("Image added successfully: {}", response),
            Err(e) => eprintln!("Error adding image: {}", e),
        }
    //     let client_id = "sisi";
    // let password = "sisira2esy";

    // match register_client(&dos_address, client_id, password).await {
    //     Ok(response) => println!("Client registered successfully: {}", response),
    //     Err(e) => eprintln!("Error registering client: {}", e),
    // }
    } else {
        eprintln!("DOS port was not determined. Exiting...");
    }
    // // // Step 1: Register a client
    // println!("{}",leader_address);
    // println!("Client: Starting the registration process...");
    //  let dos_address = "http://localhost:3030";
    

    // // Step 2: Update the client's IP address
    // if let Ok(local_ip) = get_local_ip().await {
    //     match update_client_ip(dos_address, client_id, &local_ip.to_string()).await {
    //         Ok(response) => println!("Client IP updated successfully: {}", response),
    //         Err(e) => eprintln!("Error updating client IP: {}", e),
    //     }
    // }

    // // Step 3: Add an image to the directory
    // let image_path = "image2.jpg";
    // match add_image_to_dos(dos_address, client_id, password, image_path).await {
    //     Ok(response) => println!("Image added successfully: {}", response),
    //     Err(e) => eprintln!("Error adding image: {}", e),
    // }
    // // Step 3: Add an image to the directory
    // let image_path = "image3.jpg";
    // match add_image_to_dos(dos_address, client_id, password, image_path).await {
    //     Ok(response) => println!("Image added successfully: {}", response),
    //     Err(e) => eprintln!("Error adding image: {}", e),
    // }
    // // Step 3: Add an image to the directory
    // let image_path = "image3.jpg";
    // match add_image_to_dos(dos_address, client_id, password, image_path).await {
    //     Ok(response) => println!("Image added successfully: {}", response),
    //     Err(e) => eprintln!("Error adding image: {}", e),
    // }

    // // // Step 4: Fetch composite image for the client
    // // let output_path = "composite_image.png";
    // // match fetch_composite_image(dos_address, client_id, output_path).await {
    // //     Ok(_) => println!("Composite image fetched and saved to {}", output_path),
    // //     Err(e) => eprintln!("Error fetching composite image: {}", e),
    // // }

    // // // Step 5: Delete the image from the directory
    // // let image_name = image_path; // Same name as added image
    // // match delete_image_from_dos(dos_address, client_id, password, image_name).await {
    // //     Ok(response) => println!("Image deleted successfully: {}", response),
    // //     Err(e) => eprintln!("Error deleting image: {}", e),
    // // }
    // let output_path = "composite_image2.png";
    // // Step 6: Fetch composite image again to verify deletion
    // match fetch_composite_image(dos_address, client_id, output_path).await {
    //     Ok(_) => println!("Composite image updated and saved to {}", output_path),
    //     Err(e) => eprintln!("Error fetching composite image after deletion: {}", e),
    // }

    Ok(())
}

async fn middleware_encrypt(socket: &UdpSocket, leader_addr: &str) -> tokio::io::Result<()> {
    let image_path = "image2.jpg"; // Path to the image file
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

fn resize_image(image_path: &str) -> Result<DynamicImage, Box<dyn Error>> {
    let image = ImageReader::open(image_path)?.decode()?;
    let resized_image = image.resize_exact(200, 200, image::imageops::FilterType::Triangle);
    Ok(resized_image)
}
fn encode_image_to_base64(image: &DynamicImage) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer); // Wrap Vec<u8> in Cursor
    image.write_to(&mut cursor, ImageOutputFormat::Png)?;
    Ok(base64::encode(&buffer))
}
/// Returns the local IP address of the client.
///
/// This function is now asynchronous to support `.await`.
async fn get_local_ip() -> Result<IpAddr, Box<dyn Error>> {
    // Use a UDP socket to determine the local IP address
    let socket = StdUdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?; // Connect to a public DNS server to resolve the local address
    let local_addr = socket.local_addr()?;
    Ok(local_addr.ip())
}
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
pub async fn add_image_to_dos(
    dos_address: &str,
    client_id: &str,
    password: &str,
    image_path: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let resized_image = resize_image(image_path)?;
    let base64_image = encode_image_to_base64(&resized_image)?;

    let payload = json!({
        "client_id": client_id,
        "password": password,
        "image_name": image_path,
        "image_data": base64_image
    });

    let response = client
        .post(format!("{}/add_image", dos_address))
        .json(&payload)
        .send()
        .await?;

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
pub async fn delete_image_from_dos(
    dos_address: &str,
    client_id: &str,
    password: &str,
    image_name: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "client_id": client_id,
        "password": password,
        "image_name": image_name,
    });

    let response = client
        .post(format!("{}/delete_image", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to delete image (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}
pub async fn register_client(
    dos_address: &str,
    client_id: &str,
    password: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "id": client_id,
        "password": password
    });

    let response = client
        .post(format!("{}/register_client", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to register client (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}
pub async fn update_client_ip(
    dos_address: &str,
    client_id: &str,
    current_ip: &str,
) -> Result<String, Box<dyn Error>> {
    let client = Client::new();

    let payload = json!({
        "id": client_id,
        "current_ip": current_ip
    });

    let response = client
        .post(format!("{}/update_ip", dos_address))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_text = response.text().await?;
        Ok(response_text)
    } else {
        Err(format!(
            "Failed to update IP (status: {}): {}",
            response.status(),
            response.text().await.unwrap_or_else(|_| "No response body".to_string())
        )
        .into())
    }
}

/// Receive and decode an image from the server.
async fn middleware_decrypt(socket: &UdpSocket) -> Result<(), Box<dyn Error>> {
    receive_image(socket).await?;
    decode_image("encoded_image_received.png").await?;
    Ok(())
}